//! Banco de pruebas multiagente (RNF-14): simula N agentes concurrentes
//! contra un mismo engine, sin UI ni red.
//!
//! Disponible en los tests del crate y, para otros crates, con la feature
//! `testing`.

use std::sync::Barrier;
use std::thread;

use model::{Actor, Project, ProjectId, ProjectStatus};
use serde_json::Value;

use crate::{Args, Command, Context, Cx, Engine, EngineError, SharedEngine};

pub struct Swarm {
    pub engine: SharedEngine,
    agents: Vec<Actor>,
}

impl Swarm {
    /// Registra `agents` agentes (`swarm-0`, `swarm-1`…) y los pone a
    /// trabajar sobre un engine compartido.
    pub fn new(mut engine: Engine, agents: usize) -> Result<Self, EngineError> {
        let mut actors = Vec::with_capacity(agents);
        for i in 0..agents {
            let name = format!("swarm-{i}");
            // D22: cada agente, en su propia interfaz.
            engine.dispatch(
                &Context::supervisor(),
                "interface.register",
                &Args::none().with("name", &name),
            )?;
            engine.dispatch(
                &Context::supervisor(),
                "agent.register",
                &Args::none().with("name", &name).with("interface", &name),
            )?;
            let agent = engine.read(|r| crate::lookup::agent(r, &name))?;
            actors.push(Actor::Agent(agent.id));
        }
        Ok(Self {
            engine: SharedEngine::new(engine),
            agents: actors,
        })
    }

    pub fn agents(&self) -> &[Actor] {
        &self.agents
    }

    /// Ejecuta `work` en un hilo por agente, todos arrancando a la vez para
    /// maximizar la contención. Devuelve el resultado de cada agente en orden;
    /// `Err` si su hilo entró en pánico fuera del engine.
    pub fn run<R, F>(&self, work: F) -> Vec<thread::Result<R>>
    where
        R: Send,
        F: Fn(usize, &Agent<'_>) -> R + Sync,
    {
        let start = Barrier::new(self.agents.len());
        thread::scope(|scope| {
            let handles: Vec<_> = self
                .agents
                .iter()
                .enumerate()
                .map(|(i, actor)| {
                    let (start, work) = (&start, &work);
                    let agent = Agent {
                        actor,
                        engine: &self.engine,
                    };
                    scope.spawn(move || {
                        start.wait();
                        work(i, &agent)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join()).collect()
        })
    }
}

/// Lo que ve cada agente simulado: su identidad y el engine compartido.
pub struct Agent<'a> {
    pub actor: &'a Actor,
    engine: &'a SharedEngine,
}

impl Agent<'_> {
    /// Envía un comando con un contexto nuevo atribuido a este agente.
    pub fn dispatch(&self, id: &str, args: &Args) -> Result<Value, EngineError> {
        self.engine
            .dispatch(&Context::new(self.actor.clone()), id, args)
    }
}

/// Comando que escribe en el almacenamiento y luego entra en pánico, para
/// verificar que Never Crash no deja nada a medias.
pub struct Boom;

impl Boom {
    pub const ID: &str = "test.boom";
    /// Clave del proyecto que intenta crear antes de fallar.
    pub const KEY: &str = "BOOM";
}

impl Command for Boom {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn label(&self) -> &'static str {
        "cmd-test-boom"
    }

    #[allow(clippy::panic, reason = "simula un fallo inesperado a propósito")]
    fn run(&self, cx: &mut Cx<'_>, _args: &Args) -> Result<Value, EngineError> {
        cx.tx.insert_project(&Project {
            id: ProjectId::generate(),
            key: Self::KEY.into(),
            name: "a medias".into(),
            description: String::new(),
            status: ProjectStatus::Active,
            next_number: 1,
            next_diagram: 1,
            created_at: cx.now,
            updated_at: cx.now,
            locked: false,
            workflow: Vec::new(),
            estimate_unit: model::EstimateUnit::Hours,
            fields: Vec::new(),
        })?;
        panic!("fallo simulado");
    }
}

#[cfg(test)]
mod tests {
    use store::TaskFilter;

    use super::*;

    const AGENTS: usize = 20;
    const OPS: usize = 50;

    fn engine() -> Engine {
        let mut engine = crate::tests::test_engine();
        engine
            .dispatch(
                &Context::supervisor(),
                "project.create",
                &Args::none().with("key", "SWM"),
            )
            .unwrap();
        engine
    }

    #[test]
    fn twenty_agents_create_tasks_concurrently() {
        let swarm = Swarm::new(engine(), AGENTS).unwrap();
        let results = swarm.run(|i, agent| {
            (0..OPS)
                .map(|n| {
                    agent.dispatch(
                        "task.create",
                        &Args::none()
                            .with("project", "SWM")
                            .with("title", format!("a{i}-{n}")),
                    )
                })
                .collect::<Vec<_>>()
        });
        for agent in results {
            for op in agent.unwrap() {
                op.unwrap();
            }
        }

        let mut numbers = swarm
            .engine
            .read(|r| Ok(r.tasks(&TaskFilter::default())?))
            .unwrap()
            .into_iter()
            .map(|t| t.number)
            .collect::<Vec<_>>();
        numbers.sort_unstable();
        let expected: Vec<u64> = (1..=(AGENTS * OPS) as u64).collect();
        assert_eq!(numbers, expected, "claves únicas y consecutivas");
    }

    #[test]
    fn a_panicking_agent_does_not_affect_the_others() {
        let mut engine = engine();
        engine.register(Boom);
        let swarm = Swarm::new(engine, AGENTS).unwrap();

        let results = swarm.run(|i, agent| {
            if i % 2 == 0 {
                agent.dispatch(Boom::ID, &Args::none())
            } else {
                agent.dispatch(
                    "task.create",
                    &Args::none()
                        .with("project", "SWM")
                        .with("title", format!("a{i}")),
                )
            }
        });

        for (i, result) in results.into_iter().enumerate() {
            let result = result.unwrap();
            if i % 2 == 0 {
                assert!(matches!(result, Err(EngineError::Panicked(_))));
            } else {
                result.unwrap();
            }
        }
        swarm
            .engine
            .read(|r| {
                assert!(r.project_by_key(Boom::KEY)?.is_none(), "nada a medias");
                assert_eq!(r.tasks(&TaskFilter::default())?.len(), AGENTS / 2);
                Ok(())
            })
            .unwrap();
    }
}

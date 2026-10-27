//! M11 (D15): interfaces con token, agentes sin token, cambio de identidad,
//! relevo entre ejecutor y revisores, y fases creadas por agentes.

use model::Actor;

use super::T;

/// Registra una interfaz y devuelve (token, actor de la interfaz).
fn interface(t: &mut T, name: &str, extra: &[(&str, &str)]) -> (String, Actor) {
    t.actor = Actor::Supervisor;
    let mut args = vec![("name", name)];
    args.extend_from_slice(extra);
    let out = t.run("interface.register", &args);
    let token = out["token"].as_str().unwrap().to_owned();
    let actor = t
        .engine
        .read(|r| crate::identity::from_token(r, &token))
        .unwrap();
    (token, actor)
}

fn act_as(t: &mut T, base: &Actor, name: &str) -> Actor {
    t.engine
        .read(|r| crate::identity::act_as(r, base, name))
        .unwrap()
}

#[test]
fn an_interface_creates_its_agents_and_acts_through_them() {
    let mut t = T::with_tasks(1);
    let (token, cc) = interface(&mut t, "claude-code", &[("kind", "claude-code")]);
    assert!(
        matches!(cc, Actor::Interface(_)),
        "sin agentes, actúa la interfaz"
    );

    // La interfaz crea sus agentes (sin token) pero no escribe tareas.
    t.actor = cc.clone();
    let dev = t.run(
        "agent.create",
        &[("name", "cc-dev"), ("capabilities", "rust")],
    );
    assert!(dev.get("token").is_none());
    assert_eq!(dev["interface"], "claude-code");
    t.run("agent.create", &[("name", "cc-rev")]);
    assert_eq!(
        t.fail("task.create", &[("project", "API"), ("title", "x")])
            .key(),
        "interface-needs-agent"
    );
    let mine = t.run("agent.mine", &[]);
    assert_eq!(mine["agents"].as_array().unwrap().len(), 2);

    // Con dos agentes, el token sigue siendo de la interfaz; elige con act_as.
    let again = t
        .engine
        .read(|r| crate::identity::from_token(r, &token))
        .unwrap();
    assert_eq!(again, cc);
    let dev = act_as(&mut t, &cc, "cc-dev");
    t.actor = dev.clone();
    let created = t.run(
        "task.create",
        &[("project", "API"), ("title", "por cc-dev")],
    );
    assert_eq!(created["creator_name"], "cc-dev");
    // Y puede saltar a otro agente suyo…
    let rev = act_as(&mut t, &dev, "cc-rev");
    assert_ne!(rev, dev);
    // …pero no a los de otra interfaz.
    let (_, cursor) = interface(&mut t, "cursor", &[]);
    t.actor = cursor.clone();
    t.run("agent.create", &[("name", "cu-dev")]);
    let denied = t
        .engine
        .read(|r| crate::identity::act_as(r, &dev, "cu-dev"))
        .unwrap_err();
    assert_eq!(denied.code(), "FORBIDDEN");
}

#[test]
fn the_interface_caps_level_scope_count_and_pause() {
    let mut t = T::with_tasks(1);
    t.run("project.create", &[("key", "web"), ("name", "Web")]);
    let (_, cc) = interface(
        &mut t,
        "claude-code",
        &[("level", "execute"), ("scope", "API"), ("max_agents", "2")],
    );
    // Con nivel execute la interfaz no puede ni crear agentes: los crea el supervisor.
    t.actor = cc.clone();
    assert_eq!(
        t.fail("agent.create", &[("name", "a1")]).code(),
        "FORBIDDEN"
    );
    t.actor = Actor::Supervisor;
    t.run(
        "agent.register",
        &[("name", "a1"), ("interface", "claude-code")],
    );
    t.run(
        "agent.register",
        &[("name", "a2"), ("interface", "claude-code")],
    );
    assert_eq!(
        t.fail(
            "agent.create",
            &[("name", "a3"), ("interface", "claude-code")]
        )
        .key(),
        "interface-max-agents"
    );
    let a1 = act_as(&mut t, &cc, "a1");
    t.actor = a1.clone();
    // Nivel efectivo: el de la interfaz (execute), aunque el agente sea plan.
    assert_eq!(
        t.fail("task.create", &[("project", "API"), ("title", "x")])
            .code(),
        "FORBIDDEN"
    );
    t.run("task.claim", &[("task", "API-1")]);
    // Alcance de la interfaz: nada en WEB.
    t.actor = Actor::Supervisor;
    t.run("task.create", &[("project", "WEB"), ("title", "web")]);
    t.actor = a1.clone();
    assert_eq!(
        t.fail("task.claim", &[("task", "WEB-1")]).code(),
        "FORBIDDEN"
    );
    // Pausar la interfaz pausa a sus agentes.
    t.actor = Actor::Supervisor;
    t.run("interface.pause", &[("interface", "claude-code")]);
    t.actor = a1;
    assert_eq!(
        t.fail("task.heartbeat", &[("task", "API-1")]).code(),
        "PAUSED"
    );
}

#[test]
fn agents_hand_over_through_their_columns_with_notes() {
    let mut t = T::with_tasks(2);
    let (_, cc) = interface(&mut t, "claude-code", &[]);
    t.actor = cc.clone();
    for name in ["dev", "rev", "otro"] {
        t.run("agent.create", &[("name", name)]);
    }
    let dev = act_as(&mut t, &cc, "dev");
    let rev = act_as(&mut t, &cc, "rev");
    let otro = act_as(&mut t, &cc, "otro");
    // El supervisor le pone los agentes que quiera, sin papeles (D20).
    t.actor = Actor::Supervisor;
    for a in ["dev", "rev"] {
        t.run("task.add_assignee", &[("task", "API-1"), ("agent", a)]);
    }
    t.actor = otro;
    assert_eq!(t.run("task.next", &[])["task"]["key"], "API-2");

    // dev la hace y, como pide la tarea, prepara las columnas de rev.
    t.actor = dev.clone();
    let took = t.run("task.next", &[("claim", "true")]);
    assert_eq!(took["task"]["key"], "API-1");
    for (name, base) in [("Cola de rev", "todo"), ("Trabajo de rev", "in_progress")] {
        let flow = t.run(
            "status.create",
            &[
                ("project", "API"),
                ("name", name),
                ("base", base),
                ("owner", "rev"),
            ],
        );
        assert!(flow.to_string().contains(r#""owner":"rev""#), "{flow}");
    }
    let queued = t.run(
        "task.move",
        &[
            ("task", "API-1"),
            ("status", "cola_de_rev"),
            ("note", "Hecho; pásalo a revisión"),
        ],
    );
    assert_eq!(queued["status"], "todo");
    assert_eq!(queued["stage"]["key"], "cola_de_rev");
    assert_eq!(queued["claim"], serde_json::Value::Null, "dev la suelta");

    // rev la recibe primero, con la nota, y la pasa a su mesa de trabajo.
    t.actor = rev.clone();
    let offered = t.run("task.next", &[]);
    assert_eq!(offered["task"]["key"], "API-1");
    assert!(
        offered["task"]["notes"]
            .to_string()
            .contains("pásalo a revisión")
    );
    let working = t.run(
        "task.move",
        &[("task", "API-1"), ("status", "trabajo_de_rev")],
    );
    assert_eq!(working["status"], "in_progress");
    assert_eq!(working["stage"]["key"], "trabajo_de_rev");
    assert_eq!(
        working["claim"]["agent"], "rev",
        "moverla a su mesa la toma"
    );
    // Mientras la tiene rev, dev no puede tocarla.
    t.actor = dev;
    assert_eq!(
        t.fail("task.move", &[("task", "API-1"), ("status", "done")])
            .code(),
        "CLAIMED_BY_OTHER"
    );
    // Nadie tiene que aprobarla: rev la cierra con su mensaje.
    t.actor = rev;
    let done = t.run(
        "task.move",
        &[
            ("task", "API-1"),
            ("status", "done"),
            ("note", "Revisado, todo bien"),
        ],
    );
    assert_eq!(done["status"], "done");
}

#[test]
fn a_task_takes_any_number_of_assignees_and_only_they_can_take_it() {
    let mut t = T::with_tasks(1);
    let (_, cc) = interface(&mut t, "claude-code", &[]);
    t.actor = cc.clone();
    for name in ["a", "b", "c"] {
        t.run("agent.create", &[("name", name)]);
    }
    let c = act_as(&mut t, &cc, "c");
    t.actor = Actor::Supervisor;
    for a in ["a", "b", "a"] {
        t.run("task.add_assignee", &[("task", "API-1"), ("agent", a)]);
    }
    let task = t.run("task.get", &[("task", "API-1")]);
    assert_eq!(
        task["assignees"],
        serde_json::json!(["a", "b"]),
        "sin repetir"
    );
    t.actor = c.clone();
    assert_eq!(
        t.fail("task.claim", &[("task", "API-1")]).key(),
        "task-other-assignees"
    );
    t.actor = Actor::Supervisor;
    t.run("task.remove_assignee", &[("task", "API-1"), ("agent", "a")]);
    let task = t.run("task.assign", &[("task", "API-1"), ("to", "c")]);
    assert_eq!(
        task["assignees"],
        serde_json::json!(["c"]),
        "assign deja solo a uno"
    );
    t.actor = c;
    t.run("task.claim", &[("task", "API-1")]);
}

#[test]
fn any_agent_creates_columns_for_itself() {
    let mut t = T::with_tasks(1);
    let (_, cc) = interface(&mut t, "claude-code", &[]);
    t.actor = cc.clone();
    t.run("agent.create", &[("name", "worker"), ("level", "execute")]);
    let worker = act_as(&mut t, &cc, "worker");
    t.actor = worker;
    let flow = t.run(
        "status.create",
        &[
            ("project", "API"),
            ("name", "Mesa de worker"),
            ("base", "in_progress"),
            ("owner", "worker"),
        ],
    );
    assert!(flow.to_string().contains("mesa_de_worker"), "{flow}");
    t.run(
        "status.update",
        &[
            ("project", "API"),
            ("status", "mesa_de_worker"),
            ("name", "Mi mesa"),
        ],
    );
    assert_eq!(
        t.fail(
            "status.delete",
            &[("project", "API"), ("status", "mesa_de_worker")]
        )
        .code(),
        "FORBIDDEN"
    );
    // Al tomarla, va a su propia columna de trabajo.
    let took = t.run("task.claim", &[("task", "API-1")]);
    assert_eq!(took["stage"]["key"], "mesa_de_worker");
}

#[test]
fn as_agent_switches_identity_for_one_call() {
    let mut t = T::with_tasks(1);
    let (_, cc) = interface(&mut t, "claude-code", &[]);
    t.actor = cc.clone();
    t.run("agent.create", &[("name", "uno")]);
    t.run("agent.create", &[("name", "dos")]);
    let made = t.run(
        "task.create",
        &[("project", "API"), ("title", "x"), ("as_agent", "dos")],
    );
    assert_eq!(made["creator_name"], "dos");
    t.actor = Actor::Supervisor;
    t.register(&[("name", "ajeno")]);
    t.actor = cc;
    assert_eq!(
        t.fail("task.get", &[("task", "API-1"), ("as_agent", "ajeno")])
            .key(),
        "agent-other-interface"
    );
}

/// D22: la interfaz va primero, con un nombre libre; sus agentes no tienen
/// token, respetan su máximo y su nivel, y se pueden mover a otra.
#[test]
fn interfaces_come_first_and_own_the_only_token() {
    let mut t = T::with_tasks(0);
    let made = t.run(
        "interface.register",
        &[
            ("name", "Claude Code"),
            ("level", "execute"),
            ("max_agents", "1"),
        ],
    );
    assert_eq!(made["name"], "Claude Code");
    let token = made["token"].as_str().unwrap().to_owned();
    assert_eq!(
        t.run("interface.token", &[("interface", "claude code")])["token"],
        token.as_str()
    );
    assert_eq!(
        t.fail("interface.register", &[("name", "CLAUDE  code")])
            .key(),
        "interface-name-taken"
    );
    assert_eq!(
        t.fail("interface.register", &[("name", "a/b")]).key(),
        "rule-interface-name"
    );

    let dev = t.run(
        "agent.register",
        &[
            ("name", "dev"),
            ("interface", "Claude Code"),
            ("level", "admin"),
            ("capabilities", "rust"),
        ],
    );
    assert!(dev.get("token").is_none(), "los agentes no tienen token");
    assert_eq!(dev["level"], "execute", "como mucho el de su interfaz");
    assert_eq!(
        t.fail(
            "agent.register",
            &[("name", "otro"), ("interface", "Claude Code")]
        )
        .key(),
        "interface-max-agents"
    );

    // Editar la interfaz: nombre y color; el token sigue valiendo.
    t.run(
        "interface.update",
        &[
            ("interface", "Claude Code"),
            ("name", "Claude Code Pro"),
            ("color", "#336699"),
            ("max_agents", "3"),
        ],
    );
    let got = t.run("interface.get", &[("interface", "claude code pro")]);
    assert_eq!(got["color"], "#336699");
    assert!(
        t.engine
            .read(|r| crate::identity::from_token(r, &token))
            .is_ok()
    );

    // Mover un agente a otra interfaz: se ajusta a su nivel.
    t.run(
        "interface.register",
        &[("name", "Lector"), ("level", "read")],
    );
    let moved = t.run("agent.update", &[("agent", "dev"), ("interface", "Lector")]);
    assert_eq!(moved["interface"], "Lector");
    assert_eq!(moved["level"], "read");

    // El token no sale en el historial ni en la exportación.
    let history = t.run("activity.list", &[])["items"].to_string();
    assert!(!history.contains(&token[3..]));
    let dump = t.engine.read(|r| Ok(exchange::collect(r, None)?)).unwrap();
    assert!(dump.interfaces.iter().all(|i| i.token.is_none()));
}

/// Una interfaz anterior a D22 no tiene el token guardado: hay que generar
/// uno nuevo para poder copiarlo.
#[test]
fn old_interfaces_need_a_new_token_to_copy_it() {
    use store::Store;
    let mut store = store_sqlite::SqliteStore::open_in_memory().unwrap();
    let mut tx = store.begin().unwrap();
    let now = model::Timestamp::now();
    tx.insert_interface(&model::Interface {
        id: model::InterfaceId::generate(),
        name: "vieja".into(),
        kind: "custom".into(),
        description: String::new(),
        color: "#888888".into(),
        status: model::AgentStatus::Active,
        level: model::AgentLevel::Admin,
        scope: Vec::new(),
        max_agents: 50,
        paused: false,
        token_hash: Some(foundation::secret::hash_token("tc_viejo")),
        token: None,
        created_at: now,
        updated_at: now,
    })
    .unwrap();
    tx.commit().unwrap();
    let mut t = T {
        engine: crate::Engine::new(store),
        actor: Actor::Supervisor,
        clock: None,
    };
    assert_eq!(
        t.fail("interface.token", &[("interface", "vieja")]).key(),
        "interface-token-unknown"
    );
    let new = t.run("interface.rotate_token", &[("interface", "vieja")]);
    assert_eq!(
        t.run("interface.token", &[("interface", "vieja")])["token"],
        new["token"]
    );
}

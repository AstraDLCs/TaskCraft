use i18n::Msg;
use model::{Label, LabelId};
use serde_json::Value;

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError};

command!(Create, "label.create", "cmd-label-create", create, write, [req "name": K::Text, opt "color": K::Text]);
command!(List, "label.list", "cmd-label-list", list_labels, read, [opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Update, "label.update", "cmd-label-update", update, write, [req "label": K::Text, opt "name": K::Text, opt "color": K::Text]);

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let name = rules::label_name(args.require("name")?)?;
    if cx.tx.label_by_name(&name)?.is_some() {
        return Err(EngineError::invalid(
            Msg::new("label-taken").arg("name", &name),
        ));
    }
    let color = match args.get("color") {
        Some(c) => rules::label_color(c)?,
        None => rules::default_label_color(&name),
    };
    let label = Label {
        id: LabelId::generate(),
        name,
        color,
    };
    cx.tx.insert_label(&label)?;
    Ok(serde_json::to_value(label).unwrap_or_default())
}

fn list_labels(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    paged(cx.tx.labels()?, args, |l| {
        Ok(serde_json::to_value(l).unwrap_or_default())
    })
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let reference = args.require("label")?;
    let mut label = cx.tx.label_by_name(reference)?.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-label").arg("label", reference))
    })?;
    if let Some(name) = args.get("name") {
        let name = rules::label_name(name)?;
        if !name.eq_ignore_ascii_case(&label.name) && cx.tx.label_by_name(&name)?.is_some() {
            return Err(EngineError::invalid(
                Msg::new("label-taken").arg("name", &name),
            ));
        }
        label.name = name;
    }
    if let Some(color) = args.get("color") {
        label.color = rules::label_color(color)?;
    }
    cx.tx.update_label(&label)?;
    Ok(serde_json::to_value(label).unwrap_or_default())
}

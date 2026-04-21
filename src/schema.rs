use clap::Command;
use serde_json::{json, Map, Value};

pub fn build_schema(cmd: &Command) -> Value {
    let mut root = Map::new();
    for sub in cmd.get_subcommands() {
        root.insert(sub.get_name().to_string(), command_schema(sub));
    }
    Value::Object(root)
}

fn command_schema(cmd: &Command) -> Value {
    let mut obj = Map::new();
    if let Some(about) = cmd.get_about() {
        obj.insert("about".to_string(), json!(about.to_string()));
    }

    let mut args = Map::new();
    for arg in cmd.get_arguments() {
        let name = arg.get_id().to_string();
        let mut arg_obj = Map::new();
        arg_obj.insert("required".to_string(), json!(arg.is_required_set()));
        if let Some(help) = arg.get_help() {
            arg_obj.insert("help".to_string(), json!(help.to_string()));
        }
        if let Some(default) = arg.get_default_values().first() {
            arg_obj.insert(
                "default".to_string(),
                json!(default.to_string_lossy().to_string()),
            );
        }
        let action = format!("{:?}", arg.get_action());
        arg_obj.insert("action".to_string(), json!(action));
        args.insert(name, Value::Object(arg_obj));
    }
    obj.insert("args".to_string(), Value::Object(args));

    let mut subcommands = Map::new();
    for sub in cmd.get_subcommands() {
        subcommands.insert(sub.get_name().to_string(), command_schema(sub));
    }
    if !subcommands.is_empty() {
        obj.insert("subcommands".to_string(), Value::Object(subcommands));
    }

    Value::Object(obj)
}

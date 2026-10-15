//! Feature declarations route command lines to the same actions as shortcuts.
mod history;
mod parser;
use crate::Chilpa;
use serde_json::{json, Value};
pub(crate) struct Command {
    pub name: &'static str,
    pub action: &'static str,
    pub usage: &'static str,
}
pub(crate) const fn command(
    name: &'static str,
    action: &'static str,
    usage: &'static str,
) -> Command {
    Command {
        name,
        action,
        usage,
    }
}
fn registry() -> impl Iterator<Item = &'static Command> {
    crate::queue::COMMANDS
        .iter()
        .chain(crate::ab_loop::COMMANDS)
        .chain(crate::search::COMMANDS)
        .chain(crate::navigation::COMMANDS)
        .chain(crate::preferences::COMMANDS)
        .chain(crate::hotkeys::COMMANDS)
}
pub(crate) fn for_action(id: &str) -> String {
    registry()
        .filter(|c| c.action == id)
        .map(|c| {
            format!(
                ":{}{}",
                c.name,
                if c.usage.is_empty() {
                    String::new()
                } else {
                    format!(" {}", c.usage)
                }
            )
        })
        .collect::<Vec<_>>()
        .join(" / ")
}
fn validate(line: &parser::Line) -> Result<(), String> {
    let arg = line.argument.as_str();
    let valid = match line.name.as_str() {
        "search" => !arg.is_empty(),
        "seek" | "vol" => {
            let (n, relative) = parser::number(arg, line.name == "seek")?;
            line.name == "seek" || relative || (0.0..=100.0).contains(&n)
        }
        "queue" => {
            arg.is_empty()
                || arg == "clear"
                || arg
                    .strip_prefix("save ")
                    .is_some_and(|s| !s.trim().is_empty())
        }
        "sleep" => arg == "off" || arg.parse::<u32>().is_ok_and(|n| n > 0 && n <= 10080),
        "loop" => ["a", "b", "off"].contains(&arg),
        "go" => ["album", "artist", "settings", "library"].contains(&arg),
        "set" => arg
            .split_once(char::is_whitespace)
            .is_some_and(|(key, value)| !key.is_empty() && !value.trim().is_empty()),
        _ => arg.is_empty(),
    };
    if valid {
        Ok(())
    } else {
        Err(chilpa_i18n::t("Invalid arguments; :help shows the syntax."))
    }
}
impl Chilpa {
    pub fn command_history_json(&self) -> String {
        json!(history::read(self.preference("ui.commands.history"))).to_string()
    }
    pub fn command_complete_json(&mut self, input: &str) -> String {
        let input = input.trim_start_matches([':', '/']);
        let mut options: Vec<String> = registry()
            .map(|c| format!("{}{}", c.name, if c.usage.is_empty() { "" } else { " " }))
            .collect();
        for (id, _) in self.sources() {
            options.push(format!("search:{id} "));
        }
        options.extend(["search:local ".into(), "search:all ".into()]);
        let queues: Value = serde_json::from_str(&self.listen_json()).unwrap_or_default();
        for queue in queues["tabs"].as_array().into_iter().flatten() {
            if let Some(id) = queue["id"].as_str() {
                for suffix in ["", " clear", " save "] { options.push(format!("queue:{id}{suffix}")); }
            }
        }
        for sub in [
            "queue clear",
            "queue save ",
            "sleep off",
            "loop a",
            "loop b",
            "loop off",
            "go album",
            "go artist",
            "go settings",
            "go library",
            "set keymap vi",
            "set keymap letters",
            "set keymap default",
        ] {
            options.push(sub.into());
        }
        for section in crate::settings_schema::SECTIONS {
            for group in section.groups {
                for setting in group.settings {
                    options.push(format!("set {} ", setting.key));
                }
            }
        }
        options.retain(|s| s.starts_with(input));
        options.sort();
        options.dedup();
        json!(options.into_iter().take(20).collect::<Vec<_>>()).to_string()
    }
    pub fn command_line_json(&mut self, input: &str) -> String {
        match self.prepare_command(input) {
            Ok(result) => {
                let mut lines = history::read(self.preference("ui.commands.history"));
                history::push(&mut lines, input);
                self.remember_preference("ui.commands.history", &json!(lines).to_string());
                result.to_string()
            }
            Err(error) => json!({"error": error}).to_string(),
        }
    }
    fn prepare_command(&mut self, input: &str) -> Result<Value, String> {
        let line = parser::parse(input)?;
        let Some(declaration) = registry().find(|c| c.name == line.name) else {
            return Err(chilpa_i18n::t("Unknown command; enter :help."));
        };
        validate(&line)?;
        let valid_target = match line.name.as_str() {
            "search" => ["local", "all"].contains(&line.target.as_str()) || self.sources().iter().any(|(id, _)| id == &line.target),
            "queue" => {
                let queues: Value = serde_json::from_str(&self.listen_json()).unwrap_or_default();
                queues["tabs"].as_array().is_some_and(|tabs| tabs.iter().any(|q| q["id"].as_str() == Some(line.target.as_str())))
            }
            _ => false,
        };
        if !line.target.is_empty() && !valid_target {
            return Err(chilpa_i18n::t(
                "Unknown target; use Tab to complete the source.",
            ));
        }
        let arg = line.argument.trim();
        if line.name == "set" {
            let (key, value) = arg.split_once(char::is_whitespace).unwrap();
            if key == "keymap" || key == "ui.keys.keymap" {
                if !["vi", "letters", "default"].contains(&value.trim()) {
                    return Err(chilpa_i18n::t("Choose a keymap: default, letters or vi."));
                }
                self.keys_command("keymap", value.trim());
                return Ok(json!({"action":"commands.refresh"}));
            }
            let setting = crate::settings_schema::SECTIONS
                .iter()
                .flat_map(|s| s.groups)
                .flat_map(|g| g.settings)
                .find(|s| s.key == key);
            let Some(setting) = setting else {
                return Err(chilpa_i18n::t("Unknown preference."));
            };
            let section = crate::settings_schema::SECTIONS
                .iter()
                .find(|s| {
                    s.groups
                        .iter()
                        .any(|g| g.settings.iter().any(|entry| entry.key == key))
                })
                .unwrap()
                .id;
            let (rows, _) = self.settings_rows(section, &crate::RowNotes::default());
            let row = rows.iter().find(|row| row.key == key);
            let accepted = match setting.control {
                crate::settings_schema::Control::Toggle => {
                    ["true", "false"].contains(&value.trim())
                }
                crate::settings_schema::Control::Slider => {
                    value.trim().parse::<u8>().is_ok_and(|n| n <= 100)
                }
                crate::settings_schema::Control::Select
                | crate::settings_schema::Control::Segmented
                | crate::settings_schema::Control::Device => {
                    row.is_some_and(|row| row.values.iter().any(|v| v == value.trim()))
                }
                crate::settings_schema::Control::Swatches => {
                    value.trim().len() == 7
                        && value.trim().starts_with('#')
                        && value.trim()[1..].chars().all(|c| c.is_ascii_hexdigit())
                }
                crate::settings_schema::Control::File => true,
                _ => false,
            };
            if !accepted {
                return Err(chilpa_i18n::t(
                    "Invalid preference value; check its choices in Settings.",
                ));
            }
            return Ok(json!({"action":"settings.set", "key":key, "argument":value.trim()}));
        }
        if line.name == "loop" {
            let state = self.ab_state().0;
            if arg == "b" && state != 1 {
                return Err(chilpa_i18n::t("Mark the start first with :loop a."));
            }
            if arg == "a" {
                self.ab_clear();
                self.ab_mark();
            }
            if arg == "b" {
                self.ab_mark();
            }
            if arg == "off" {
                self.ab_clear();
            }
            if arg != "off" && self.ab_state().0 != if arg == "a" { 1 } else { 2 } {
                return Err(chilpa_i18n::t(
                    "The loop cannot be marked here; its end needs at least two seconds.",
                )
                .into());
            }
        }
        if line.name == "go"
            && ((arg == "album" && self.now.album_id == 0)
                || (arg == "artist" && self.now.artist.is_empty()))
        {
            return Err(chilpa_i18n::t("The current track has no such destination."));
        }
        if line.name == "vol" && self.player_volume().locked {
            return Err(self.player_volume().why);
        }
        let mut result = json!({"action":declaration.action,"argument":arg,"target":line.target});
        if line.name == "seek" || line.name == "vol" {
            let (n, relative) = parser::number(arg, line.name == "seek")?;
            result["value"] = json!(n);
            result["relative"] = json!(relative);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_declarations_are_unique_and_reference_actions() {
        let mut names = std::collections::HashSet::new();
        for c in registry() {
            assert!(names.insert(c.name));
            assert!(crate::hotkeys::has_action(c.action), "{}", c.action);
        }
    }
    #[test]
    fn invalid_arguments_cannot_reach_an_action() {
        for input in [
            "play unexpected",
            "queue erase",
            "queue save",
            "sleep -1",
            "loop c",
            "go nowhere",
            "vol 101",
            "seek NaN",
            "set key",
        ] {
            assert!(validate(&parser::parse(input).unwrap()).is_err(), "{input}");
        }
        for input in [
            "queue save Night songs",
            "seek +10",
            "seek 1:30",
            "sleep off",
            "set keymap vi",
            "go album",
        ] {
            assert!(validate(&parser::parse(input).unwrap()).is_ok(), "{input}");
        }
    }
}

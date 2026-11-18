//! Tags: every tag with its totals, and renaming or deleting one everywhere.

use crate::{Api, Reply, Result, bad, decode, internal, not_found, undo};
use serde::Deserialize;
use serde_json::{Value, json};
use tcgbinder_core::collection::{TagEdit, clean_user_tag, is_system_tag, same_tag, tag_summaries};
use tcgbinder_core::service::GameService;

/// A tag rename's or delete's answer: `changed`, and `undo` for its backup.
fn tag_edit_answer(g: &GameService, edit: &TagEdit) -> Value {
    let mut answer = json!({ "changed": edit.changed });
    if let Some(b) = &edit.backup {
        answer["undo"] = undo(g, b);
    }
    answer
}

impl Api {
    /// Every tag in use with its totals over the copies that carry it, and the
    /// untagged copies' totals (null when every copy is tagged).
    pub(crate) fn tags(&self, g: &GameService) -> Reply {
        Reply::Json(json!(tag_summaries(&g.store.cards())))
    }

    /// Renames a tag everywhere: `{"name": "<new>"}`. Returns `changed` and `undo`.
    pub(crate) fn rename_tag(&self, g: &GameService, name: &str, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            name: String,
        }
        let req: Req = decode(body)?;
        let to = clean_user_tag(&req.name).map_err(|e| {
            if is_system_tag(req.name.trim()) {
                bad(e.to_string())
            } else {
                bad(format!("{:?} can't be a tag: at most 40 characters, with no comma or semicolon", req.name.trim()))
            }
        })?;
        self.tag_in_use(g, name)?;
        let edit = g.store.rename_tag(name, &to).map_err(internal)?;
        tracing::info!(game = g.game.id(), from = name, to, changed = edit.changed, "tag renamed");
        Ok(Reply::Json(tag_edit_answer(g, &edit)))
    }

    /// Removes a tag from every purchase. Returns `changed` and `undo`.
    pub(crate) fn delete_tag(&self, g: &GameService, name: &str) -> Result {
        self.tag_in_use(g, name)?;
        let edit = g.store.delete_tag(name).map_err(internal)?;
        tracing::info!(game = g.game.id(), tag = name, changed = edit.changed, "tag deleted");
        Ok(Reply::Json(tag_edit_answer(g, &edit)))
    }

    pub(crate) fn tag_in_use(&self, g: &GameService, name: &str) -> Result<()> {
        if g.store.tag_names().iter().any(|t| same_tag(t, name)) {
            Ok(())
        } else {
            Err(not_found(format!("no card has the tag {:?}", name)))
        }
    }
}

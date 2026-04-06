//! Epic management commands.
//!
//! All CRUD operations go through the DB (sole source of truth).
//! Markdown is only used for spec body content (`specs/*.md`) and reviews.

mod audit;
mod crud;
mod deps;
mod helpers;
mod history;
mod lifecycle;
mod types;

pub use history::{cmd_diff, cmd_replay};
pub use types::EpicCmd;

/// Dispatch an epic subcommand.
pub fn dispatch(cmd: &EpicCmd, json: bool) {
    match cmd {
        EpicCmd::Create { title, branch } => crud::cmd_create(title, branch, json),
        EpicCmd::Plan { id, file } => crud::cmd_set_plan(id, file, json),
        EpicCmd::Review { id, status } => crud::cmd_set_plan_review_status(id, status, json),
        EpicCmd::Completion { id, status } => {
            crud::cmd_set_completion_review_status(id, status, json)
        }
        EpicCmd::Branch { id, name } => crud::cmd_set_branch(id, name, json),
        EpicCmd::Title { id, title } => crud::cmd_set_title(id, title, json),
        EpicCmd::Close {
            id,
            skip_gap_check,
        } => lifecycle::cmd_close(id, *skip_gap_check, json),
        EpicCmd::Reopen { id } => lifecycle::cmd_reopen(id, json),
        EpicCmd::Archive { id, force } => lifecycle::cmd_archive(id, *force, json),
        EpicCmd::Clean => lifecycle::cmd_clean(json),
        EpicCmd::Audit { id, force } => audit::cmd_audit(id, *force, json),
        EpicCmd::AddDep { epic, depends_on } => deps::cmd_add_dep(epic, depends_on, json),
        EpicCmd::RmDep { epic, depends_on } => deps::cmd_rm_dep(epic, depends_on, json),
        EpicCmd::SetBackend {
            id,
            impl_spec,
            review,
            sync,
        } => crud::cmd_set_backend(id, impl_spec, review, sync, json),
        EpicCmd::AutoExec {
            id,
            pending,
            done,
        } => crud::cmd_set_auto_execute(id, *pending, *done, json),
    }
}

//! Scanning the project's folder and git history for its stored secret
//! values. The scan runs in the background with only the values' lengths
//! and hashes (`ferrumenv_core::leaks::Fingerprints`); results name the
//! file, commit and variable, never the value, followed by guidance.
//! Nothing is rewritten.

use ferrum_ui::prelude::*;
use ferrumenv_core::leaks::{self, Finding, Fingerprints};

use super::Dialog;
use crate::app::EnvApp;
use crate::widgets::{muted, primary_button};

pub(crate) struct LeakScanDialog {
    pub(crate) project_id: String,
    /// `None` while the scan runs.
    pub(crate) result: Option<Result<Vec<Finding>, String>>,
}

impl EnvApp {
    pub(crate) fn open_leak_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.vault().and_then(|u| u.project()) else {
            return;
        };
        let Some(dir) = project.path.clone() else {
            self.show_error(
                "Link the project to its folder (open the folder with ⌥⌘O) to scan it.",
                cx,
            );
            return;
        };
        let project_id = project.id.clone();
        let prints = Fingerprints::new(project);
        let dialog = LeakScanDialog {
            project_id: project_id.clone(),
            result: None,
        };
        self.open_dialog(Dialog::LeakScan(dialog), Vec::new(), None, window, cx);
        if prints.is_empty() {
            self.finish_leak_scan(&project_id, Ok(Vec::new()), cx);
            return;
        }
        let task = cx.background_spawn(async move {
            let mut findings = leaks::scan_working_tree(&dir, &prints);
            if leaks::is_git_repo(&dir) {
                findings.extend(leaks::scan_history(&dir, &prints).map_err(|e| e.to_string())?);
            }
            Ok(findings)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.finish_leak_scan(&project_id, result, cx)
            })
            .ok();
        })
        .detach();
    }

    fn finish_leak_scan(
        &mut self,
        project_id: &str,
        result: Result<Vec<Finding>, String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(Dialog::LeakScan(d)) = self.dialog_mut()
            && d.project_id == project_id
        {
            d.result = Some(result);
        }
        cx.notify();
    }

    pub(crate) fn render_leak_scan(&self, d: &LeakScanDialog, cx: &mut Context<Self>) -> Modal {
        let modal = self
            .modal("Scan for leaked secrets", cx)
            .width(px(640.))
            .child(muted(format!(
                "Looks for this project's secret values ({}+ characters) in its folder (files git would track) and in every commit of its git history, by hash. Values are never shown.",
                leaks::MIN_LEN
            )))
            .footer(primary_button("leak-scan-done", "Done", cx, |this, window, cx| {
                this.close_dialog(window, cx);
            }));
        let findings = match &d.result {
            None => return modal.child(Label::new("Scanning…")),
            Some(Err(e)) => return modal.child(Label::new(e.clone()).color(Color::Error)),
            Some(Ok(findings)) if findings.is_empty() => {
                return modal.child(
                    Label::new("No stored secret values found.")
                        .size(LabelSize::Small)
                        .color(Color::Success),
                );
            }
            Some(Ok(findings)) => findings,
        };
        let mut list = v_flex()
            .id("leak-findings")
            .max_h(px(260.))
            .overflow_y_scroll()
            .gap_1();
        for f in findings {
            let place = match (f.line, &f.commit) {
                (Some(line), _) => format!("{}:{line}", f.path),
                (None, Some(commit)) => format!("{} · commit {commit}", f.path),
                (None, None) => f.path.clone(),
            };
            list = list.child(
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(format!("{} ({})", f.label.key, f.label.environment))
                            .size(LabelSize::Small)
                            .color(Color::Warning),
                    )
                    .child(muted(place).truncate()),
            );
        }
        modal
            .child(
                Label::new(format!("{} place(s) hold a stored secret:", findings.len()))
                    .color(Color::Warning),
            )
            .child(list)
            .child(muted(leaks::GUIDANCE))
    }
}

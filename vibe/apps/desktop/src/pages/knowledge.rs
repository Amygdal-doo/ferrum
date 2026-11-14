//! Knowledge Center: the project's rule files FerrumChat puts in the
//! agent's prompt (AGENTS.md, CLAUDE.md, `.ferrum/rules/*.md`,
//! `.cursor/rules/*.mdc`), what each does, Open in Ferrum Editor, and New
//! rule (`.ferrum/rules/<name>.md`).

use std::path::{Path, PathBuf};

use ferrum_ui::components::IconSize;
use ferrum_ui::prelude::*;
use ferrumagent_ui::core::rules::{Rule, RuleSource};

use super::{field, frame, placeholder, row, section, two_lines};
use crate::app::FerrumVibe;
use crate::nav::Page;

/// When a rule reaches the agent, in a few words.
#[must_use]
pub fn what_it_does(rule: &Rule) -> String {
    if let Some(scope) = &rule.scope {
        return format!("Applied when files in {}/ are in context", scope.display());
    }
    if rule.always {
        return "Always in the agent's instructions".into();
    }
    if !rule.globs.is_empty() {
        return format!("Applied to files matching {}", rule.globs.join(", "));
    }
    match &rule.description {
        Some(d) => format!("Listed for the agent to read when relevant: {d}"),
        None => "Not applied (no globs or description)".into(),
    }
}

fn source_label(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Agents => "AGENTS.md",
        RuleSource::Claude => "Claude",
        RuleSource::Ferrum => "Ferrum",
        RuleSource::Cursor => "Cursor",
    }
}

/// `Use Tailwind!` → `use-tailwind.md`; `None` without letters or digits.
#[must_use]
pub fn rule_file_name(name: &str) -> Option<String> {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    (!slug.is_empty()).then(|| format!("{slug}.md"))
}

/// Creates `.ferrum/rules/<name>.md` with frontmatter; an existing file is
/// left alone (and returned).
pub fn create_rule(root: &Path, name: &str) -> Result<PathBuf, String> {
    let file_name = rule_file_name(name).ok_or("Give the rule a name.")?;
    let dir = root.join(".ferrum").join("rules");
    let file = dir.join(file_name);
    if file.exists() {
        return Ok(file);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = format!(
        "---\ndescription: {}\nalwaysApply: true\nglobs:\n---\n\n# {}\n\n- \n",
        name.trim(),
        name.trim()
    );
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file)
}

impl FerrumVibe {
    fn new_rule(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        let name = self.pages.rule_name.read(cx).text().to_string();
        match create_rule(&root, &name) {
            Ok(file) => {
                self.pages
                    .rule_name
                    .update(cx, |input, cx| input.set_text("", cx));
                self.open_file(&file, Some(1), cx);
                self.pages
                    .refresh(Page::Knowledge, Some(&root), &self.agent_home, None, cx);
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
        cx.notify();
    }

    pub(crate) fn knowledge_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = frame(
            Page::Knowledge,
            "Rules and project notes the agent follows. They are read at the start of every turn.",
            cx,
        );
        let Some(root) = self.project.clone() else {
            return page
                .child(placeholder("Open a project to see its rules.", cx))
                .into_any_element();
        };
        page = page.child(
            h_flex()
                .gap_2()
                .child(field(&self.pages.rule_name, cx).max_w(px(360.)))
                .child(
                    Button::new("new-rule", "New Rule")
                        .style(ButtonStyle::Filled)
                        .size(ButtonSize::Compact)
                        .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                        .on_click(cx.listener(|this, _, _, cx| this.new_rule(cx))),
                ),
        );
        page = page.child(section("In this project", cx));
        if self.pages.rules.is_empty() {
            page = page.child(placeholder(
                "No rules yet. Add AGENTS.md, or create one above.",
                cx,
            ));
        }
        for (i, rule) in self.pages.rules.iter().enumerate() {
            let file = root.join(&rule.path);
            page = page.child(
                row(("rule", i), cx)
                    .child(Icon::new(IconName::Bookmark).color(Color::Muted))
                    .child(two_lines(rule.path.clone(), what_it_does(rule)))
                    .child(
                        Label::new(source_label(rule.source))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new(("open-rule", i), "Open")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_file(&file, None, cx);
                            })),
                    ),
            );
        }
        page.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(always: bool, globs: &[&str], description: Option<&str>) -> Rule {
        Rule {
            path: "x.md".into(),
            source: RuleSource::Ferrum,
            description: description.map(str::to_string),
            always,
            globs: globs.iter().map(|g| (*g).to_string()).collect(),
            body: String::new(),
            scope: None,
        }
    }

    #[test]
    fn rules_say_when_they_apply() {
        assert!(what_it_does(&rule(true, &[], None)).starts_with("Always"));
        assert!(what_it_does(&rule(false, &["*.ts", "*.tsx"], None)).contains("*.ts, *.tsx"));
        assert!(what_it_does(&rule(false, &[], Some("DB tips"))).contains("DB tips"));
        let mut nested = rule(true, &[], None);
        nested.scope = Some(PathBuf::from("web"));
        assert!(what_it_does(&nested).contains("web/"));
    }

    #[test]
    fn new_rules_get_a_slug_and_frontmatter() {
        assert_eq!(
            rule_file_name(" Use Tailwind! ").as_deref(),
            Some("use-tailwind.md")
        );
        assert_eq!(
            rule_file_name("API / errors").as_deref(),
            Some("api-errors.md")
        );
        assert_eq!(rule_file_name("  !! "), None);

        let dir = tempfile::tempdir().expect("tempdir");
        let file = create_rule(dir.path(), "Testing").expect("create");
        assert!(file.ends_with(".ferrum/rules/testing.md"));
        std::fs::write(&file, "---\ndescription: mine\n---\nkeep").expect("write");
        // Creating it again keeps the user's text.
        assert_eq!(create_rule(dir.path(), "testing").expect("again"), file);
        let rules = ferrumagent_ui::core::rules::load(dir.path());
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].description.as_deref(), Some("mine"));
        assert!(create_rule(dir.path(), "").is_err());
    }
}

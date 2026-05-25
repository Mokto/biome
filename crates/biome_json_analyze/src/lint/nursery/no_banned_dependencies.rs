use biome_analyze::{
    Ast, Rule, RuleDiagnostic, RuleSource, context::RuleContext, declare_lint_rule,
};
use biome_console::markup;
use biome_deserialize::DeserializableValue;
use biome_json_syntax::JsonRoot;
use biome_module_replacements::{
    ModuleReplacement, find_mapping, find_replacement, resolve_doc_url,
};
use biome_rowan::TextRange;
use biome_rule_options::no_banned_dependencies::NoBannedDependenciesOptions;

use crate::utils::is_package_json;

declare_lint_rule! {
    /// Disallow dependencies that are known to have better alternatives.
    ///
    /// This rule checks `dependencies` and `devDependencies` in `package.json`
    /// against e18e's replacement data and suggests modern, native, or more
    /// maintainable alternatives.
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```json,expect_diagnostic
    /// {
    ///   "dependencies": {
    ///     "npm-run-all": "^4.1.5"
    ///   }
    /// }
    /// ```
    ///
    /// ### Valid
    ///
    /// ```json
    /// {
    ///   "dependencies": {
    ///     "unknown-module": "^1.0.0"
    ///   }
    /// }
    /// ```
    ///
    pub NoBannedDependencies {
        version: "next",
        name: "noBannedDependencies",
        language: "json",
        recommended: false,
        sources: &[RuleSource::EslintE18e("ban-dependencies").same()],
    }
}

impl Rule for NoBannedDependencies {
    type Query = Ast<JsonRoot>;
    type State = RuleState;
    type Signals = Vec<Self::State>;
    type Options = NoBannedDependenciesOptions;

    fn run(ctx: &RuleContext<Self>) -> Self::Signals {
        let mut found = Vec::new();

        let path = ctx.file_path();
        if !is_package_json(path) {
            return found;
        }

        let root = ctx.query();
        let Some(value) = root.value().ok() else {
            return found;
        };
        let Some(object) = value.as_json_object_value() else {
            return found;
        };

        for member in object.json_member_list() {
            let Some(member) = member.ok() else {
                continue;
            };
            let Some(name) = member.name().ok() else {
                continue;
            };
            let Some(name_text) = name.inner_string_text() else {
                continue;
            };
            let Some(name_text) = name_text.ok() else {
                continue;
            };
            if !DEPENDENCY_KEYS.contains(&name_text.text()) {
                continue;
            }

            let Some(dep_value) = member.value().ok() else {
                continue;
            };
            let Some(dep_object) = dep_value.as_json_object_value() else {
                continue;
            };

            for member in dep_object.json_member_list() {
                let Some(member) = member.ok() else {
                    continue;
                };
                let Some(name) = member.name().ok() else {
                    continue;
                };
                let Some(name) = name.as_json_member_name() else {
                    continue;
                };
                let Some(name_text) = name.inner_string_text().ok() else {
                    continue;
                };

                let Some(mapping) = find_mapping(name_text.text()) else {
                    continue;
                };
                let Some(replacement) = mapping
                    .replacements
                    .iter()
                    .find_map(|replacement_id| find_replacement(replacement_id))
                else {
                    continue;
                };

                let kind = match replacement {
                    ModuleReplacement::Native(replacement) => ReplacementKind::Native {
                        replacement: replacement.common.id,
                    },
                    ModuleReplacement::Documented(replacement) => ReplacementKind::Documented {
                        replacement: replacement.replacement_module,
                    },
                    ModuleReplacement::Simple(replacement) => ReplacementKind::Simple {
                        description: replacement.description,
                    },
                    ModuleReplacement::Removal(replacement) => ReplacementKind::Removal {
                        description: replacement.description,
                    },
                };

                let url = resolve_doc_url(mapping.url).or_else(|| {
                    let url = match replacement {
                        ModuleReplacement::Native(replacement) => Some(replacement.url),
                        ModuleReplacement::Documented(replacement) => replacement.url,
                        ModuleReplacement::Simple(replacement) => replacement.url,
                        ModuleReplacement::Removal(replacement) => replacement.url,
                    };
                    resolve_doc_url(url)
                });

                found.push(RuleState {
                    dep_name: mapping.module_name,
                    kind,
                    range: name.range(),
                    url,
                });
            }
        }

        found
    }

    fn diagnostic(_ctx: &RuleContext<Self>, state: &Self::State) -> Option<RuleDiagnostic> {
        let mut diagnostic = match &state.kind {
            ReplacementKind::Native { replacement } => RuleDiagnostic::new(
                rule_category!(),
                state.range,
                markup! {
                    "The dependency "<Emphasis>{state.dep_name}</Emphasis>" should be replaced with native functionality."
                },
            )
            .note(markup! {
                "Use "<Emphasis>{replacement}</Emphasis>" instead."
            }),
            ReplacementKind::Documented { replacement } => RuleDiagnostic::new(
                rule_category!(),
                state.range,
                markup! {
                    "The dependency "<Emphasis>{state.dep_name}</Emphasis>" should be replaced with an alternative package."
                },
            )
            .note(markup! {
                "Prefer "<Emphasis>{replacement}</Emphasis>" for this use case."
            }),
            ReplacementKind::Simple { description } => RuleDiagnostic::new(
                rule_category!(),
                state.range,
                markup! {
                    "The dependency "<Emphasis>{state.dep_name}</Emphasis>" should be replaced with inline or local logic."
                },
            )
            .note(markup! {{description}}),
            ReplacementKind::Removal { description } => RuleDiagnostic::new(
                rule_category!(),
                state.range,
                markup! {
                    "The dependency "<Emphasis>{state.dep_name}</Emphasis>" is flagged as no longer needed."
                },
            )
            .note(markup! {{description}}),
        };

        if let Some(url) = &state.url {
            diagnostic = diagnostic.note(markup! {
                "Read more: "<Hyperlink href={url.as_str()}>{url.as_str()}</Hyperlink>
            });
        }

        Some(diagnostic)
    }
}

const DEPENDENCY_KEYS: &[&str] = &["dependencies", "devDependencies"];

enum ReplacementKind {
    Native { replacement: &'static str },
    Documented { replacement: &'static str },
    Simple { description: &'static str },
    Removal { description: &'static str },
}

pub struct RuleState {
    dep_name: &'static str,
    kind: ReplacementKind,
    range: TextRange,
    url: Option<String>,
}

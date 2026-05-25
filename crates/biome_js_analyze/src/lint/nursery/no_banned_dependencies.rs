use biome_analyze::{Rule, RuleDiagnostic, RuleSource, context::RuleContext, declare_lint_rule};
use biome_console::markup;
use biome_js_syntax::AnyJsImportLike;
use biome_module_replacements::{
    ModuleReplacement, find_mapping, find_replacement, resolve_doc_url,
};
use biome_rowan::{AstNode, TextRange};
use biome_rule_options::no_banned_dependencies::NoBannedDependenciesOptions;

use crate::{services::manifest::Manifest, utils::parse_package_name};

declare_lint_rule! {
    /// Disallow dependencies that are known to have better alternatives.
    ///
    /// This rule checks static imports, dynamic `import()`, and `require()` calls
    /// and suggests modern, native, or more maintainable alternatives based on
    /// e18e's replacement data.
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```js,expect_diagnostic
    /// import runAll from "npm-run-all";
    /// ```
    ///
    /// ### Valid
    ///
    /// ```js
    /// import something from "unknown-module";
    /// ```
    ///
    pub NoBannedDependencies {
        version: "next",
        name: "noBannedDependencies",
        language: "js",
        recommended: false,
        sources: &[RuleSource::EslintE18e("ban-dependencies").same()],
    }
}

impl Rule for NoBannedDependencies {
    type Query = Manifest<AnyJsImportLike>;
    type State = RuleState;
    type Signals = Option<Self::State>;
    type Options = NoBannedDependenciesOptions;

    fn run(ctx: &RuleContext<Self>) -> Self::Signals {
        let node = ctx.query();
        if node.is_in_ts_module_declaration() {
            return None;
        }

        let source = node.inner_string_text()?;
        let dep_name = parse_package_name(source.text())?;

        let mapping = find_mapping(dep_name)?;
        let replacement = mapping
            .replacements
            .iter()
            .find_map(|replacement_id| find_replacement(replacement_id))?;

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

        let range = node
            .module_name_token()
            .map_or_else(|| node.range(), |token| token.text_trimmed_range());

        Some(RuleState {
            dep_name: mapping.module_name,
            kind,
            range,
            url,
        })
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

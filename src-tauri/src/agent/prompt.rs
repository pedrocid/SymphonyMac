use crate::orchestrator::{PipelineStage, RunConfig, StageContext};
use std::collections::HashMap;

/// Render a custom template by replacing `{{variable}}` placeholders.
fn render_template(
    template: &str,
    issue_number: u64,
    repo: &str,
    issue_title: &str,
    issue_body: &str,
    attempt: u32,
    previous_error: &str,
) -> String {
    template
        .replace("{{issue_number}}", &issue_number.to_string())
        .replace("{{repo}}", repo)
        .replace("{{issue_title}}", issue_title)
        .replace(
            "{{issue_body}}",
            &issue_body.chars().take(4000).collect::<String>(),
        )
        .replace("{{attempt}}", &attempt.to_string())
        .replace("{{previous_error}}", previous_error)
}

fn default_prompt(stage: &PipelineStage) -> &'static str {
    match stage {
        PipelineStage::Implement => "\
You are working on GitHub issue #{{issue_number}} in repository {{repo}}.

Title: {{issue_title}}

Description:
{{issue_body}}

Instructions:
1. Analyze the issue carefully
2. Implement the fix or feature with clean, well-structured code
3. Commit your changes with a descriptive message
4. Create a Pull Request:
   gh pr create --title \"Fix #{{issue_number}}: {{issue_title}}\" --body \"Closes #{{issue_number}}\"

Do NOT run tests - that will be handled in a later stage.",

        PipelineStage::CodeReview => "\
You are a code reviewer for repository {{repo}}.

A Pull Request has been created for issue #{{issue_number}}: {{issue_title}}

Instructions:
1. Run this command to find the PR for issue #{{issue_number}}:
   gh pr list --state open --json number,title,headRefName
2. Check out the PR branch
3. Review ALL changed files carefully. Look for:
   - Bugs, logic errors, edge cases
   - Security issues
   - Code style and readability
   - Missing error handling
   - Performance issues
4. If you find issues, FIX them directly in the code, commit, and push
5. If the code looks good or after fixing issues, leave a summary comment on the PR:
   gh pr comment <PR_NUMBER> --body \"Code review completed. <summary of findings and fixes>\"

Be thorough but practical. Fix real problems, don't nitpick style.",

        PipelineStage::Testing => "\
You are a test engineer for repository {{repo}}.

A Pull Request for issue #{{issue_number}}: {{issue_title}} has been reviewed and is ready for testing.

Issue description:
{{issue_body}}

Instructions:
1. Run this command to find the PR for issue #{{issue_number}}:
   gh pr list --state open --json number,title,headRefName
2. Check out the PR branch
3. Identify the project type and run the appropriate test commands:
   - Node.js: npm test or npm run test
   - Python: pytest or python -m pytest
   - Rust: cargo test
   - Go: go test ./...
   - Swift: swift test
   - Or check package.json / Makefile / README for test instructions
4. If tests fail:
   - Analyze the failures
   - Fix the issues in the code
   - Commit and push the fixes
   - Re-run tests to confirm they pass
5. END-TO-END TESTING (CRITICAL):
   After existing tests pass, you MUST perform end-to-end validation:
   a) Read the issue title and description above carefully to understand what was fixed or added.
   b) If the issue describes a specific bug or feature:
      - Reproduce the original scenario described in the issue to verify the fix works end-to-end.
      - For bugs: try to trigger the original bug and confirm it no longer occurs.
      - For features: exercise the new feature through its intended usage path.
      - Use the project's actual entry points (CLI commands, API endpoints, scripts, UI) to test, not just unit tests.
   c) If the issue is too abstract or there is no specific scenario to reproduce:
      - Perform a quick smoke test: build the project and run its main entry point to verify nothing is broken.
      - For web apps: start the dev server and verify it loads without errors.
      - For CLI tools: run the main command with --help or a basic invocation.
      - For libraries: run a quick import/usage check.
   d) If E2E testing reveals issues, fix them, commit, push, and re-test.
6. Comment on the PR with your findings:
   gh pr comment <PR_NUMBER> --body \"Testing completed. Unit tests: PASS. E2E validation: <describe what you tested and results>. Ready to merge.\"

Make sure ALL tests pass and E2E validation succeeds before finishing.",

        PipelineStage::Merge => "\
You are a release engineer for repository {{repo}}.

A Pull Request for issue #{{issue_number}}: {{issue_title}} has passed code review and all tests.

Instructions:
1. Run this command to find the PR for issue #{{issue_number}}:
   gh pr list -R {{repo}} --state open --json number,title,headRefName
2. Check out the PR branch and update it against the base branch to detect conflicts BEFORE merging:
   gh pr checkout <PR_NUMBER> -R {{repo}}
   git fetch origin main && git rebase origin/main
3. If there are merge conflicts:
   - Resolve the conflicts in the affected files
   - Run: git add <resolved_files> && git rebase --continue
   - Push the updated branch: git push --force-with-lease
4. Merge the PR into the default branch:
   gh pr merge <PR_NUMBER> -R {{repo}} --merge --delete-branch
5. Confirm the merge was successful by checking:
   gh pr view <PR_NUMBER> -R {{repo}} --json state
   The state MUST be \"MERGED\". If it is not, the merge failed.
6. Close the issue if it wasn't auto-closed:
   gh issue close {{issue_number}} -R {{repo}}

IMPORTANT: If the merge fails due to conflicts that you cannot resolve, \
exit with a non-zero exit code so the pipeline knows the merge did not succeed.",

        PipelineStage::Done => "",
    }
}

pub(crate) fn build_prompt(
    stage: &PipelineStage,
    issue_number: u64,
    repo: &str,
    issue_title: &str,
    issue_body: &str,
    stage_prompts: &HashMap<String, String>,
    attempt: u32,
    previous_error: &str,
    previous_context: Option<&StageContext>,
) -> String {
    let stage_key = stage.to_string();
    let template = match stage_prompts.get(&stage_key) {
        Some(custom) if !custom.trim().is_empty() => custom.as_str(),
        _ => default_prompt(stage),
    };
    let mut rendered = render_template(
        template,
        issue_number,
        repo,
        issue_title,
        issue_body,
        attempt,
        previous_error,
    );

    if let Some(context) = previous_context {
        rendered = format!("{}\n\n{}", rendered, context.to_prompt_section());
    }

    if !previous_error.is_empty() && !template.contains("{{previous_error}}") {
        format!(
            "{}\n\nIMPORTANT: Previous attempt ({}) failed with: {}\nFix the issues and try again.",
            rendered, attempt, previous_error
        )
    } else {
        rendered
    }
}

/// Returns the default prompt templates for all stages.
pub fn get_default_prompts() -> HashMap<String, String> {
    let stages = [
        PipelineStage::Implement,
        PipelineStage::CodeReview,
        PipelineStage::Testing,
        PipelineStage::Merge,
    ];
    stages
        .into_iter()
        .map(|stage| (stage.to_string(), default_prompt(&stage).to_string()))
        .collect()
}

/// Create a short display string for the command being run (truncates the prompt).
pub(crate) fn format_command_display(cmd: &str, args: &[String]) -> String {
    let binary = std::path::Path::new(cmd)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| cmd.to_string());
    let display_args: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.len() > 80 {
                let truncated: String = arg.chars().take(77).collect();
                format!("\"{}...\"", truncated)
            } else if arg.contains(' ') {
                format!("\"{}\"", arg)
            } else {
                arg.clone()
            }
        })
        .collect();
    format!("{} {}", binary, display_args.join(" "))
}

pub(crate) fn build_command_args(config: &RunConfig, prompt: &str) -> (String, Vec<String>) {
    match config.agent_type.as_str() {
        "codex" => {
            let mut args = vec!["exec".to_string()];
            if config.auto_approve {
                args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
            }
            if let Some(home) = dirs::home_dir() {
                let gh_config = home.join(".config/gh");
                if gh_config.exists() {
                    args.push("--add-dir".to_string());
                    args.push(gh_config.to_string_lossy().to_string());
                }
            }
            args.push(prompt.to_string());
            (crate::paths::resolve("codex"), args)
        }
        "custom" => build_custom_command_args(&config.custom_agent_command, prompt),
        _ => {
            let mut args = vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
            ];
            if config.auto_approve {
                args.push("--dangerously-skip-permissions".to_string());
            }
            args.push(prompt.to_string());
            (crate::paths::resolve("claude"), args)
        }
    }
}

/// Parse a custom agent command template and substitute the prompt.
///
/// The template is split on whitespace. The first token is resolved as the
/// binary (searching the usual PATH dirs). Remaining tokens become arguments.
/// If any token contains `{{prompt}}`, the placeholder is replaced with the
/// actual prompt text. If no token contains the placeholder, the prompt is
/// appended as the final argument.
fn build_custom_command_args(template: &str, prompt: &str) -> (String, Vec<String>) {
    let tokens: Vec<&str> = template.split_whitespace().collect();
    if tokens.is_empty() {
        // Fallback to claude if the user left the command empty.
        return (
            crate::paths::resolve("claude"),
            vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
                prompt.to_string(),
            ],
        );
    }

    let binary = crate::paths::resolve(tokens[0]);
    let has_placeholder = tokens[1..].iter().any(|t| t.contains("{{prompt}}"));

    let mut args: Vec<String> = tokens[1..]
        .iter()
        .map(|t| t.replace("{{prompt}}", prompt))
        .collect();

    if !has_placeholder {
        args.push(prompt.to_string());
    }

    (binary, args)
}

#[cfg(test)]
mod tests {
    use super::build_prompt;
    use crate::orchestrator::PipelineStage;
    use std::collections::HashMap;

    #[test]
    fn gh_pr_lookup_prompts_do_not_include_a_trailing_pipe() {
        for stage in [
            PipelineStage::CodeReview,
            PipelineStage::Testing,
            PipelineStage::Merge,
        ] {
            let prompt = build_prompt(
                &stage,
                57,
                "pedrocid/SymphonyMac",
                "Split agent.rs into focused pipeline and process modules",
                "",
                &HashMap::new(),
                1,
                "",
                None,
            );

            assert!(!prompt.contains("headRefName |"));
            assert!(!prompt.contains("headRefName to find"));
        }
    }

    #[test]
    fn test_build_prompt_appends_previous_context_and_retry_error() {
        use crate::orchestrator::StageContext;

        let previous_context = StageContext {
            from_stage: "implement".to_string(),
            files_changed: vec!["src/App.tsx".to_string()],
            lines_added: 12,
            lines_removed: 3,
            pr_number: Some(91),
            branch_name: Some("symphony/issue-62".to_string()),
            summary: "Implemented automated coverage.".to_string(),
        };

        let prompt = build_prompt(
            &PipelineStage::Testing,
            62,
            "pedrocid/SymphonyMac",
            "Add automated coverage",
            "Cover the Tauri, React, and pipeline flows.",
            &HashMap::new(),
            2,
            "cargo test failed",
            Some(&previous_context),
        );

        assert!(prompt.contains("issue #62"));
        assert!(prompt.contains("## Context from implement stage"));
        assert!(prompt.contains("PR number: #91"));
        assert!(prompt.contains("Previous attempt (2) failed with: cargo test failed"));
    }

    #[test]
    fn test_build_prompt_uses_custom_stage_template() {
        let mut stage_prompts = HashMap::new();
        stage_prompts.insert(
            "testing".to_string(),
            "Custom test plan for {{repo}} issue #{{issue_number}}".to_string(),
        );

        let prompt = build_prompt(
            &PipelineStage::Testing,
            62,
            "pedrocid/SymphonyMac",
            "Add automated coverage",
            "Cover the Tauri, React, and pipeline flows.",
            &stage_prompts,
            1,
            "",
            None,
        );

        assert_eq!(
            prompt,
            "Custom test plan for pedrocid/SymphonyMac issue #62"
        );
    }

    #[test]
    fn test_custom_command_with_placeholder() {
        let (bin, args) =
            super::build_custom_command_args("aider --yes-always {{prompt}}", "fix the bug");
        assert!(bin.contains("aider"));
        assert_eq!(args, vec!["--yes-always", "fix the bug"]);
    }

    #[test]
    fn test_custom_command_without_placeholder() {
        let (bin, args) =
            super::build_custom_command_args("my-agent --flag", "do something");
        assert!(bin.contains("my-agent"));
        assert_eq!(args, vec!["--flag", "do something"]);
    }

    #[test]
    fn test_custom_command_empty_falls_back_to_claude() {
        let (bin, args) = super::build_custom_command_args("", "hello");
        assert!(bin.contains("claude"));
        assert!(args.contains(&"hello".to_string()));
    }
}

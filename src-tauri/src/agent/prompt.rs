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
1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
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
1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
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
1. Run: gh pr list -R {{repo}} --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
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

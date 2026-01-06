# SDD

This repo uses lightweight role profiles to reduce ambiguity and regressions.

## Structure
- `.sdd/roles/` role profiles (architect, implementer, reviewer, qa)
- `.sdd/tasks/` task templates and task files
- `.sdd/sdd.sh` / `.sdd/sdd.ps1` compose a prompt for a role + task

## How to use
1) Create a task file from the template:
   - `.sdd/tasks/task-template.md` -> `.sdd/tasks/<task-name>.md`
2) Compose the prompt for the role you want:
   - Mac/Linux/WSL: `./.sdd/sdd.sh role <role> .sdd/tasks/<task-name>.md`
   - Windows: `powershell -ExecutionPolicy Bypass -File .sdd/sdd.ps1 -Role <role> -TaskFile .sdd/tasks/<task-name>.md`
3) Paste the output into the assistant.

## Manual invocation (chat)
If you want to skip the scripts, send:
- "Act as <RoleName>"
- Paste the role file
- Paste the task file
- "Follow the role instructions and produce output in the role's required format."

## Auto-select rules
If no role is specified, choose the best fit:
- Architect: asked to plan, design, specify interfaces, assess risks, or clarify scope.
- Implementer: asked to change code, implement features, fix bugs, or update tests.
- Reviewer: asked to review changes, look for issues, or give a verdict.
- QA: asked to test, break a feature, or define test strategy/fixtures.
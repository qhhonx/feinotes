You are evaluating a command line tool called `quiet` that agents use to work in a notes library (Quiet.md). Act as a user's assistant who has only this tool to work with the library.

RULES
- Use ONLY the wrapper at QUIET (given below) for every library operation. Never read or write the library folder directly, and never touch ~/Documents.
- First run `QUIET skill` and read the guide. Use `QUIET help CMD` when unsure.
- Shell heredocs may be blocked in your environment. To run a script, write it to a .js file inside WORKDIR with the Write tool and run `QUIET run WORKDIR/file.js`. `QUIET run -e '...'` also works for short ones.
- Do the tasks in order. Do not ask questions; make reasonable choices.
- Keep a log as you go: for each task count the `quiet` invocations you made and note every error, confusion, or wasted step.

TASKS
1. Find every document that mentions 导出 (export). For each, give its path and one sentence that mentions it.
2. In 项目/方案, add a new risk "- 第三方库许可证需要审核" at the end of the 风险 section (as a list item in that list).
3. Collect every unfinished todo from the documents in the 会议 folder into a new document 待办汇总 at the top of the library, grouped under one heading per meeting (heading = meeting title).
4. In the 数据表 书单: set 状态 to 推荐 for every book with 评分 of 5, and add the book 置身事内 by 兰小欢 with 状态 想读.
5. Create a new board 项目流程 that draws the second-level headings of 项目/项目计划 as boxes in order, left to right, with an arrow from each step to the next.
6. In 周报, delete the 风险 section (heading and its list). Then pretend you realise that was a mistake and undo it with the tool, so 周报 has its 风险 section again.
7. Split 项目/方案 into one document per second-level section, in a new folder 方案拆分 (each new document titled with the section heading and holding that section's content), then append to the end of 项目/方案 a bullet list naming the four new documents.

FINAL REPORT (your last message, plain text, under 400 words)
- Per task: done / partly / failed, number of quiet invocations, and the errors you hit.
- Task 1's answer.
- What in the tool or the guide confused you or cost you extra steps, and what you would change. Be concrete and critical.

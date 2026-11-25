# Evaluating `quiet` with agents

Measures whether agents can use the CLI from its guide alone: each agent
gets `quiet skill`, the seven tasks in `tasks.md` and a library of its own,
and `check.py` then reads what it left behind. Results and what changed
because of them are in [docs/design/CLI.md](../../docs/design/CLI.md#评估).

```sh
cargo build --release -p quiet_cli
tools/cli-eval/build.sh /tmp/quiet-eval/haiku/lib
```

Give each agent a wrapper so that it can only reach its own library:

```sh
printf '#!/bin/sh\nexport QUIET_MD_HOME=%s\nexec %s "$@"\n' /tmp/quiet-eval/haiku/lib "$PWD/target/release/quiet" > /tmp/quiet-eval/haiku/quiet
chmod +x /tmp/quiet-eval/haiku/quiet
```

Prompt each agent with `tasks.md`, giving the wrapper as QUIET and a scratch
folder as WORKDIR. Ask for the final report the file describes. Then:

```sh
python3 tools/cli-eval/check.py /tmp/quiet-eval/*/lib
```

The last check, mentions in task 7, is a bonus: the task asks only for a list of the new documents.

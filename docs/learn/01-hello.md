# 1. Hello, PlainText

**Goal:** Run your first program and know how PlainText talks back when something is wrong.

PlainText programs are ordinary text files ending in `.pt`. You do not compile them yourself —
you hand them to the `plaintext` tool.

## Your first program

Create `hello.pt` next to your terminal:

```plaintext
print("Hello, world!")
```

Run it:

```bash
plaintext run hello.pt
```

You should see:

```
Hello, world!
```

`print` writes to the console. You can pass several values: `print("a", 1, true)`.

## Comments

Comments are notes for humans. PlainText ignores them.

```plaintext
// the rest of this line is a comment

print("hi")   /* block comments work too */
```

## Catch mistakes early

```bash
plaintext check hello.pt
```

`check` type-checks without running. If something is wrong you get a **file**, **line**,
**message**, and often a **hint**. Prefer `check` while you edit; use `run` when you want
to see behavior.

## The REPL

```bash
plaintext repl
```

Type an expression, press Enter, see the result. Type `exit` to leave. Great for trying
`2 + 2` or `print("hi")` without creating a file.

## Common mistakes

| What you see | Likely cause |
|--------------|--------------|
| Weird parse errors on new examples | Old `plaintext` — run `plaintext version` ([fix](../troubleshooting.md)) |
| Nothing happens | You typed `plaintext hello.pt` — need `plaintext run hello.pt` |

## Practice

1. Change the message inside `print(...)` and run again.
2. Introduce a typo on purpose, run `plaintext check`, and read the error.
3. Open the REPL and evaluate `print(1 + 2)`.

**Next:** [Variables and text →](02-variables-and-text.md)

---

[← Docs home](../README.md)

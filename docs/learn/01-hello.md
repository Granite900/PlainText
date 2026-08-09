# 1. Hello, PlainText

PlainText programs are plain text files ending in `.pt`. You run them with the `plaintext`
command.

## Your first program

Create a file `hello.pt`:

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

## Comments

Notes for humans. PlainText ignores them.

```plaintext
// one line

print("hi")   /* also a comment */
```

## Check before you run

```bash
plaintext check hello.pt
```

If something is wrong, PlainText prints a clear error with a file, line, and hint.

## Try it

- Change the message inside `print(...)`.
- Run `plaintext repl`, type `print("hi")`, press Enter.

**Next:** [Variables and text →](02-variables-and-text.md)

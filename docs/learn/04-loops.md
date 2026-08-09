# 4. Loops

```plaintext
for every score in scores {
    print(score)
}

repeat 3 times {
    print("go")
}

while count is more than 0 {
    decrease count by 1
}

loop {
    if done {
        break
    }
}
```

Use `break` to leave a loop and `continue` to skip to the next round.

> Loops do **not** create a new scope. Only functions do. A name you make inside a loop is
> still there afterward.

## Try it

The loop section of [`examples/basics.pt`](../../examples/basics.pt).

**Next:** [Functions →](05-functions.md)

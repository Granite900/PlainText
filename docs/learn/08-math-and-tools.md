# 8. Math and the toolbox

## Math needs an import

```plaintext
import math

print(sqrt(144))
print(greatest(3, 9, 5))
print(pi)
```

Without `import math`, names like `sqrt`, `random_between`, `pi`, and `e` are not available.

## Console input and exit

```plaintext
name = input("What is your name? ")
print("Hi, {name}!")

exit()      // stop the program (optional status code 0–255)
```

## Try it

- Math: [`examples/stdlib.pt`](../../examples/stdlib.pt)
- Asking questions: [`examples/ask.pt`](../../examples/ask.pt)
- List tools + `exit`: [`examples/list_tools.pt`](../../examples/list_tools.pt)

**Next:** [Modules →](09-modules.md)

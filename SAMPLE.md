# Table
| Level  | Description     |
|--------|-----------------|
| S Tier | Gold Gym        |
| A Tier | Anytime Fitness |
| B Tier | Jexer           |

# TaskList
- [ ] Task1
- [ ] Task2
    - [ ] detail task1.1
    - [ ] detail task1.2
- [ ] Task3

# Strike through
~~Hi~~ Hello, ~there~ world!

This ~~has a

new paragraph~~.

This will ~~~not~~~ strike.


# Fenced Code
```
<
>
```

```rust
fn main() {
    println!("hello world!");
}
```

# AutoLink
www.commonmark.org

Visit www.commonmark.org/help for more information.


Visit www.commonmark.org.

Visit www.commonmark.org/a.b.

www.google.com/search?q=Markup+(business)

www.google.com/search?q=Markup+(business)))

(www.google.com/search?q=Markup+(business))

(www.google.com/search?q=Markup+(business)

# In Page Link
## Example headings

### Sample Section

### This'll be a _Helpful_ Section About the Greek Letter Θ!
A heading containing characters not allowed in fragments, UTF-8 characters, two consecutive spaces between the first and second words, and formatting.

### This heading is not unique in the file

TEXT 1

### This heading is not unique in the file

TEXT 2

## Links to the example headings above

Link to the sample section: [Link Text](#sample-section).

Link to the helpful section: [Link Text](#thisll-be-a-helpful-section-about-the-greek-letter-Θ).

Link to the first non-unique section: [Link Text](#this-heading-is-not-unique-in-the-file).

Link to the second non-unique section: [Link Text](#this-heading-is-not-unique-in-the-file-1).

# Alert
> [!NOTE]
> Useful information that users should know, even when skimming content.

> [!TIP]
> Helpful advice for doing things better or more easily.

> [!IMPORTANT]
> Key information users need to know to achieve their goal.

> [!WARNING]
> Urgent info that needs immediate user attention to avoid problems.

> [!CAUTION]
> Advises about risks or negative outcomes of certain actions.

# Color model
`#0969DA`

`rgb(9, 105, 218)`

`hsl(212, 92%, 45%)`

# Emoji

@octocat :+1: This PR looks great - it's ready to merge! :shipit:

# Footnote
Here is a simple footnote[^1].

A footnote can also have multiple lines[^2].

[^1]: My reference.
[^2]: To add line breaks within a footnote, add 2 spaces to the end of a line.  
This is a second line.

# MathJax
## Inline Math
this sentence is uses `$` delimiters to show math inline: $\sqrt{3x-1}+{1+x}^2$

This sentence uses $\` and \`$ delimiters to show math inline: $`\sqrt{3x-1}+(1+x)^2`$


## Block Math
**The Cauchy-Schwarz Inequality**\
$$\left( \sum_{k=1}^n a_k b_k \right)^2 \leq \left( \sum_{k=1}^n a_k^2 \right) \left( \sum_{k=1}^n b_k^2 \right)$$


**The Cauchy-Schwarz Inequality**
```math
\left( \sum_{k=1}^n a_k b_k \right)^2 \leq \left( \sum_{k=1}^n a_k^2 \right) \left( \sum_{k=1}^n b_k^2 \right)
```

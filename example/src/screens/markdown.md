## GPUI Kit - TextView

![Alpine lake beneath snow-capped mountains](https://gpui.example/alpine-lake.jpg)

🚀 **你好，世界。** Build with *Rust* and `gpui-kit`.

**Bold**、*斜体*、~~删除线~~、`行内代码`与[文档](https://gpui-kit.com)。

> **原生体验，跨越平台。** Rendered by GPUI.

- **自由组合 · Compose naturally**
  - 图文混排与 *nested content*。

```rust
TextView::markdown("hello", "# gpui-kit")
    .selectable(true)
```

| Feature | Example | State |
| :--- | :---: | ---: |
| **文字** | *中英混排* | Ready |
| `Code` | `42` | Done |
| 图片 | 山间湖泊 | Loaded |

---

### More cases

- [x] Completed task
- [ ] Pending task

1. An ordered item with **bold** text.
2. A second item with a nested quote:

   > Nested blocks keep their indentation.

Escaped: \*literal\*, \[brackets\], &lt;tag&gt;, &amp;. Backticks: `` `value` ``.

### Nested quote

> First level.
>
> > Second level with **bold** and *italic*.
>
> Back to the first level.

### Code inside a fence

````markdown
```json
{"enabled": true, "items": [1, 2, 3]}
```
````

### Wrapping table

| Case | Content |
| --- | --- |
| Long cell | A longer description tests wrapping within a narrow table cell. |
| Mixed | **Bold**, *italic*, ~~removed~~, `code` |
| Pipe | Left \| right |
| Empty | |

### Line breaks

A hard break follows here.  
This starts a new line.

A long **bold phrase with enough words to cross a line boundary**, followed by *italic text* and `inline_code`.

#### Heading four

##### Heading five

###### Heading six

**End of document.**

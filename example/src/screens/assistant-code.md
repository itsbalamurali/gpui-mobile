### Keep content and controls separate

Use **TextView** for the answer body. Put the thinking state above it and reply actions below it.

```rust
let reply = TextView::markdown(
    "assistant-reply",
    content,
)
.w_full()
.selectable(true)
.scrollable(false);

Message::new().content(
    MessageContent::new()
        .child(reply),
)
```

Here, `content` is the Markdown string for one reply.

- **Selection** lets readers keep a sentence or a code sample.
- **One scroll container** keeps the conversation moving as a single page.
- **Stable message IDs** keep each reply associated with its own content.

Use a distinct ID for every message in a history. The fixed ID above is for one reply.

Explore the [GPUI Kit documentation](https://gpui-kit.com) when you’re ready to compose the rest of the interface.

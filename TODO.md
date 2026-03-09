- Improve the UX of the APIs exposed by the library.
- Assitive technologies support (e.g., screen readers, voice control).
- Deeplinking?
- Provide inbuilt routing/navigation solution ?
- Maps?
- In-app purchases?
- Assets management?
- Google Fonts?

- Use the new Component View System from GPUI once its merged and rewrite the UI and packages using the new system. This will allow us to have a more modular and reusable codebase, and also make it easier to maintain and extend the library in the future. https://github.com/zed-industries/zed/pull/51030

Screens to implement:

https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/examples/gif_viewer.rs
https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/examples/image_gallery.rs
https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/examples/image_loading.rs

Update the text inputs with all the functionality from here https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/examples/input.rs

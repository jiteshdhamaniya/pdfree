# PDFree

A simple, fast macOS app to remove password protection from PDFs. Built with [Tauri](https://tauri.app/) and native macOS Core Graphics.

## Download

**[Download PDFree.dmg (latest release)](../../releases/latest/download/PDFree.dmg)**

> Requires macOS 12 or later (Apple Silicon & Intel)

### Important: First launch

Since PDFree is not signed with an Apple Developer certificate, macOS Gatekeeper will block it on first open. To fix this, run the following command in Terminal after copying to Applications:

```bash
xattr -cr /Applications/PDFree.app
```

Then open PDFree normally — it will work fine after that.

## Features

- **Drag & drop** — Drop one or more password-protected PDFs
- **Batch unlock** — Unlock multiple PDFs at once, each with its own password
- **Native performance** — Uses macOS Core Graphics for fast, reliable decryption
- **Reveal in Finder / Open File** — Quick actions after unlocking
- **Lightweight** — ~2.6MB DMG, no external dependencies
- **Privacy-first** — Everything runs locally, zero network requests

## How to use

1. Open PDFree
2. Drag a password-protected PDF onto the window
3. Enter the PDF password
4. Click **Unlock** (or press Enter)
5. Done — the unlocked file is saved as `<filename>_unlocked.pdf` in the same folder

## Build from source

**Prerequisites:**
- [Rust](https://rustup.rs/)
- macOS 12+

```bash
# Install Tauri CLI
cargo install tauri-cli

# Clone and build
git clone https://github.com/jiteshdhamaniya/pdfree.git
cd pdfree
cargo tauri build

# The app is at src-tauri/target/release/bundle/macos/PDFree.app
```

## How it works

PDFree uses macOS's native **Core Graphics** framework (`CGPDFDocument`) to:

1. Open the encrypted PDF with the provided password
2. Create a new PDF context without encryption
3. Draw each page from the original into the new document
4. Save the result — a fully unlocked PDF

Supports all PDF encryption types that macOS supports (RC4, AES-128, AES-256).

## Author

Made by [Jitesh Dhamaniya](https://github.com/jiteshdhamaniya)

## License

MIT — see [LICENSE](LICENSE) for details.

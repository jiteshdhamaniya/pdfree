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
- **Auto-unlock** — Saved passwords are tried automatically, so most statements open with zero typing
- **Auto-unlock profile** — Enter your details once (name, date of birth, card last-4, …) and PDFree generates the password formats banks & credit cards use for statements, then tries them for you
- **Statement recognition** — Recognizes the issuer from the filename (ICICI, HDFC, SBI Card, Axis, and more)
- **Secure storage** — Saved passwords and your profile live in the **macOS Keychain**, encrypted and only on your Mac — never in a plaintext file, never sent anywhere
- **Native performance** — Uses macOS Core Graphics for fast, reliable decryption
- **Reveal in Finder / Open File** — Quick actions after unlocking
- **Lightweight** — no external dependencies
- **Privacy-first** — Everything runs locally, zero network requests

## How to use

1. Open PDFree
2. Drag a password-protected PDF onto the window
3. PDFree tries your saved passwords and auto-unlock profile automatically. If one matches, the file is unlocked instantly.
4. If nothing matches, enter the password and click **Unlock** (or press Enter). You can then save it for next time.
5. Done — the unlocked file is saved as `<filename>_unlocked.pdf` in the same folder

### Auto-unlock for bank & credit-card statements

Statement PDFs usually use a password built from your personal details — for example, the first 4 letters of your name plus your date of birth. Open the **🔑 Saved** panel and fill in the **Auto-unlock profile**. PDFree generates the common formats (ICICI, HDFC, SBI Card, Axis, and others) and tries them automatically, so your statements open without typing a password. Your details are stored encrypted in the macOS Keychain and are only ever used locally on your Mac.

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

For auto-unlock, saved passwords and your profile are stored as encrypted items in the macOS Keychain. When you drop a PDF, PDFree tries (in order) your saved passwords, then passwords generated from your profile, then an empty password — falling back to manual entry only if none work. Generated passwords exist only in memory during the unlock attempt and are never written to disk or sent over the network.

## Author

Made by [Jitesh Dhamaniya](https://github.com/jiteshdhamaniya)

## License

MIT — see [LICENSE](LICENSE) for details.

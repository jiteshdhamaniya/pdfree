// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::CString;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use core_foundation::base::TCFType;
use core_foundation::url::CFURL;
use serde::{Deserialize, Serialize};

// Core Graphics types
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type CGPDFDocumentRef = *const std::ffi::c_void;
type CGPDFPageRef = *const std::ffi::c_void;
type CGContextRef = *const std::ffi::c_void;
type CFURLRef = *const std::ffi::c_void;
type CFDictionaryRef = *const std::ffi::c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPDFDocumentCreateWithURL(url: CFURLRef) -> CGPDFDocumentRef;
    fn CGPDFDocumentRelease(document: CGPDFDocumentRef);
    fn CGPDFDocumentIsEncrypted(document: CGPDFDocumentRef) -> bool;
    fn CGPDFDocumentUnlockWithPassword(document: CGPDFDocumentRef, password: *const i8) -> bool;
    fn CGPDFDocumentIsUnlocked(document: CGPDFDocumentRef) -> bool;
    fn CGPDFDocumentGetNumberOfPages(document: CGPDFDocumentRef) -> usize;
    fn CGPDFDocumentGetPage(document: CGPDFDocumentRef, page_number: usize) -> CGPDFPageRef;
    fn CGPDFPageGetBoxRect(page: CGPDFPageRef, box_type: i32) -> CGRect;
    fn CGPDFContextCreateWithURL(
        url: CFURLRef,
        media_box: *const CGRect,
        auxiliary_info: CFDictionaryRef,
    ) -> CGContextRef;
    fn CGPDFContextClose(context: CGContextRef);
    fn CGContextRelease(context: CGContextRef);
    fn CGContextBeginPage(context: CGContextRef, media_box: *const CGRect);
    fn CGContextEndPage(context: CGContextRef);
    fn CGContextDrawPDFPage(context: CGContextRef, page: CGPDFPageRef);
}

const KCGPDF_MEDIA_BOX: i32 = 0;

// ===================== Keychain-backed password vault =====================
//
// Saved passwords are stored as a single JSON blob in the macOS Keychain
// (a generic-password item), so the user's secrets are encrypted at rest and
// never written to a plaintext file on disk.

const KC_SERVICE: &str = "com.pdfree.app";
const KC_ACCOUNT: &str = "saved-passwords";
const KC_PROFILE_ACCOUNT: &str = "auto-unlock-profile";

#[derive(Serialize, Deserialize, Clone)]
struct SavedPassword {
    id: String,
    label: String,
    password: String,
}

/// Metadata sent to the frontend — deliberately excludes the password value
/// so secrets stay in the backend.
#[derive(Serialize, Clone)]
struct SavedPasswordMeta {
    id: String,
    label: String,
}

#[derive(Serialize)]
struct UnlockResult {
    output_path: String,
    /// Label of the saved password that worked, or `None` when the PDF wasn't
    /// encrypted (no password needed).
    matched_label: Option<String>,
}

/// Sentinel error returned when an encrypted PDF couldn't be opened with any
/// saved password — the frontend uses this to fall back to manual entry.
const NEEDS_PASSWORD: &str = "NEEDS_PASSWORD";

fn load_vault() -> Vec<SavedPassword> {
    match security_framework::passwords::get_generic_password(KC_SERVICE, KC_ACCOUNT) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn persist_vault(vault: &[SavedPassword]) -> Result<(), String> {
    let json = serde_json::to_vec(vault).map_err(|e| e.to_string())?;
    security_framework::passwords::set_generic_password(KC_SERVICE, KC_ACCOUNT, &json)
        .map_err(|e| format!("Keychain error: {}", e))
}

fn metas(vault: Vec<SavedPassword>) -> Vec<SavedPasswordMeta> {
    vault
        .into_iter()
        .map(|p| SavedPasswordMeta {
            id: p.id,
            label: p.label,
        })
        .collect()
}

// ===================== Auto-unlock profile (formula engine) =====================
//
// The user stores their personal "tokens" once (name, DOB, card last-4, …).
// From those we generate the password formats that banks/card issuers commonly
// use for statement PDFs, and try them all automatically. Like the vault, the
// profile lives encrypted in the Keychain and never touches the frontend except
// when the user is editing it.

#[derive(Serialize, Deserialize, Clone, Default)]
struct Profile {
    #[serde(default)]
    name: String,
    /// ISO date `YYYY-MM-DD`.
    #[serde(default)]
    dob: String,
    #[serde(default)]
    card_last4: String,
    #[serde(default)]
    account_last4: String,
    #[serde(default)]
    customer_id: String,
    #[serde(default)]
    pan: String,
    #[serde(default)]
    mobile: String,
}

fn load_profile() -> Profile {
    match security_framework::passwords::get_generic_password(KC_SERVICE, KC_PROFILE_ACCOUNT) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Profile::default(),
    }
}

fn persist_profile(profile: &Profile) -> Result<(), String> {
    let json = serde_json::to_vec(profile).map_err(|e| e.to_string())?;
    security_framework::passwords::set_generic_password(KC_SERVICE, KC_PROFILE_ACCOUNT, &json)
        .map_err(|e| format!("Keychain error: {}", e))
}

fn alpha_only(s: &str) -> String {
    s.chars().filter(|c| c.is_alphabetic()).collect()
}

fn digits_only(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// First `n` alphabetic characters of the first word of `s`.
fn first_n_alpha(s: &str, n: usize) -> String {
    let first_word = s.split_whitespace().next().unwrap_or("");
    alpha_only(first_word).chars().take(n).collect()
}

/// Last `n` digits found in `s`.
fn last_n_digits(s: &str, n: usize) -> String {
    let d = digits_only(s);
    if d.len() > n {
        d[d.len() - n..].to_string()
    } else {
        d
    }
}

/// Build the list of candidate passwords from a profile, covering the common
/// Indian bank / credit-card statement formats (and a few generic ones).
fn generate_candidates(p: &Profile) -> Vec<String> {
    // Name fragments in the casings issuers use.
    let mut name4: Vec<String> = Vec::new();
    let raw4 = first_n_alpha(&p.name, 4);
    if !raw4.is_empty() {
        let upper = raw4.to_uppercase();
        let lower = raw4.to_lowercase();
        let mut title = lower.clone();
        if let Some(c) = title.get_mut(0..1) {
            c.make_ascii_uppercase();
        }
        for v in [upper, lower, title] {
            if !name4.contains(&v) {
                name4.push(v);
            }
        }
    }

    // Date-of-birth fragments in every common ordering.
    let mut dob_parts: Vec<String> = Vec::new();
    let bits: Vec<&str> = p.dob.split('-').collect();
    if bits.len() == 3 {
        let (yyyy, mm, dd) = (bits[0], bits[1], bits[2]);
        let yy = if yyyy.len() == 4 { &yyyy[2..] } else { yyyy };
        if dd.len() == 2 && mm.len() == 2 {
            dob_parts.push(format!("{}{}", dd, mm)); // DDMM
            dob_parts.push(format!("{}{}", mm, dd)); // MMDD
            dob_parts.push(format!("{}{}{}", dd, mm, yy)); // DDMMYY
            dob_parts.push(format!("{}{}{}", dd, mm, yyyy)); // DDMMYYYY
            dob_parts.push(format!("{}{}{}", yyyy, mm, dd)); // YYYYMMDD
        }
        if yyyy.len() == 4 {
            dob_parts.push(yyyy.to_string());
        }
    }

    let card4 = last_n_digits(&p.card_last4, 4);
    let acct4 = last_n_digits(&p.account_last4, 4);
    let mob4 = last_n_digits(&p.mobile, 4);
    let pan_raw: String = p.pan.chars().filter(|c| c.is_alphanumeric()).collect();
    let cust = p.customer_id.trim().to_string();

    let mut out: Vec<String> = Vec::new();
    let push = |s: String, out: &mut Vec<String>| {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };

    // name-only and dob-only
    for n in &name4 {
        push(n.clone(), &mut out);
    }
    for d in &dob_parts {
        push(d.clone(), &mut out);
    }

    // name + dob  (ICICI / HDFC / Axis / SBI Card style)
    for n in &name4 {
        for d in &dob_parts {
            push(format!("{}{}", n, d), &mut out);
        }
    }

    // name + card/account last 4
    for n in &name4 {
        push(format!("{}{}", n, card4), &mut out);
        push(format!("{}{}", n, acct4), &mut out);
    }

    // PAN (statements often use PAN, or PAN + DOB)
    if !pan_raw.is_empty() {
        push(pan_raw.to_uppercase(), &mut out);
        push(pan_raw.to_lowercase(), &mut out);
        for d in &dob_parts {
            push(format!("{}{}", pan_raw.to_uppercase(), d), &mut out);
            push(format!("{}{}", pan_raw.to_lowercase(), d), &mut out);
        }
    }

    // Customer / relationship ID, mobile last 4
    push(cust.clone(), &mut out);
    push(cust.to_uppercase(), &mut out);
    push(mob4, &mut out);

    out
}

// ===================== Core Graphics helpers =====================

fn open_doc(input_path: &str) -> Result<CGPDFDocumentRef, String> {
    let input_url =
        CFURL::from_path(Path::new(input_path), false).ok_or("Failed to create input URL")?;
    let doc = unsafe { CGPDFDocumentCreateWithURL(input_url.as_concrete_TypeRef() as CFURLRef) };
    if doc.is_null() {
        return Err("Failed to open PDF file".to_string());
    }
    Ok(doc)
}

/// Render an already-unlocked document into a fresh, unencrypted PDF.
/// Does NOT release `doc` — the caller owns it.
fn render_doc_to(doc: CGPDFDocumentRef, output_path: &str) -> Result<(), String> {
    let page_count = unsafe { CGPDFDocumentGetNumberOfPages(doc) };
    if page_count == 0 {
        return Err("PDF has no pages".to_string());
    }

    let output_url =
        CFURL::from_path(Path::new(output_path), false).ok_or("Failed to create output URL")?;

    let context = unsafe {
        CGPDFContextCreateWithURL(
            output_url.as_concrete_TypeRef() as CFURLRef,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if context.is_null() {
        return Err("Failed to create output PDF".to_string());
    }

    for i in 1..=page_count {
        let page = unsafe { CGPDFDocumentGetPage(doc, i) };
        if page.is_null() {
            continue;
        }
        let media_box = unsafe { CGPDFPageGetBoxRect(page, KCGPDF_MEDIA_BOX) };
        unsafe {
            CGContextBeginPage(context, &media_box);
            CGContextDrawPDFPage(context, page);
            CGContextEndPage(context);
        }
    }

    unsafe {
        CGPDFContextClose(context);
        CGContextRelease(context);
    }

    Ok(())
}

/// Try to unlock `doc` with `password`. Returns true on success.
fn try_password(doc: CGPDFDocumentRef, password: &str) -> bool {
    match CString::new(password) {
        Ok(c) => unsafe { CGPDFDocumentUnlockWithPassword(doc, c.as_ptr()) },
        Err(_) => false,
    }
}

fn output_path_for(file_path: &str) -> String {
    let input = Path::new(file_path);
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let parent = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .to_string();
    format!("{}/{}_unlocked.pdf", parent, stem)
}

// ===================== Tauri commands =====================

#[tauri::command]
fn list_passwords() -> Vec<SavedPasswordMeta> {
    metas(load_vault())
}

#[tauri::command]
fn save_password(label: String, password: String) -> Result<Vec<SavedPasswordMeta>, String> {
    if password.is_empty() {
        return Err("Password cannot be empty".to_string());
    }
    let mut vault = load_vault();
    // Skip if we already store this exact password.
    if !vault.iter().any(|p| p.password == password) {
        let label = {
            let t = label.trim();
            if t.is_empty() {
                "Saved password".to_string()
            } else {
                t.to_string()
            }
        };
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            .to_string();
        vault.push(SavedPassword {
            id,
            label,
            password,
        });
        persist_vault(&vault)?;
    }
    Ok(metas(vault))
}

#[tauri::command]
fn delete_password(id: String) -> Result<Vec<SavedPasswordMeta>, String> {
    let mut vault = load_vault();
    vault.retain(|p| p.id != id);
    persist_vault(&vault)?;
    Ok(metas(vault))
}

#[tauri::command]
fn get_profile() -> Profile {
    load_profile()
}

#[tauri::command]
fn save_profile(profile: Profile) -> Result<(), String> {
    persist_profile(&profile)?;
    Ok(())
}

/// Manual unlock with a single user-supplied password.
#[tauri::command]
fn unlock_pdf(file_path: String, password: String) -> Result<String, String> {
    let doc = open_doc(&file_path)?;

    let unlocked = if unsafe { CGPDFDocumentIsEncrypted(doc) } {
        try_password(doc, &password)
    } else {
        true
    };

    if !unlocked || !unsafe { CGPDFDocumentIsUnlocked(doc) } {
        unsafe { CGPDFDocumentRelease(doc) };
        return Err("Wrong password".to_string());
    }

    let output_path = output_path_for(&file_path);
    let res = render_doc_to(doc, &output_path);
    unsafe { CGPDFDocumentRelease(doc) };
    res?;
    Ok(output_path)
}

/// Auto unlock: try every saved password (and an empty password) against the
/// PDF. Returns `NEEDS_PASSWORD` if none worked so the UI can prompt manually.
#[tauri::command]
fn unlock_pdf_auto(file_path: String) -> Result<UnlockResult, String> {
    let doc = open_doc(&file_path)?;

    let mut matched_label = None;

    if unsafe { CGPDFDocumentIsEncrypted(doc) } {
        let mut ok = false;
        // 1. Exact saved passwords.
        for p in load_vault() {
            if try_password(doc, &p.password) {
                matched_label = Some(p.label);
                ok = true;
                break;
            }
        }
        // 2. Passwords generated from the user's auto-unlock profile.
        if !ok {
            for cand in generate_candidates(&load_profile()) {
                if try_password(doc, &cand) {
                    matched_label = Some("from your profile".to_string());
                    ok = true;
                    break;
                }
            }
        }
        // 3. Some PDFs are encrypted but openable with an empty user password.
        if !ok && try_password(doc, "") {
            ok = true; // matched_label stays None — no real password was needed
        }
        if !ok {
            unsafe { CGPDFDocumentRelease(doc) };
            return Err(NEEDS_PASSWORD.to_string());
        }
    }

    if !unsafe { CGPDFDocumentIsUnlocked(doc) } {
        unsafe { CGPDFDocumentRelease(doc) };
        return Err(NEEDS_PASSWORD.to_string());
    }

    let output_path = output_path_for(&file_path);
    let res = render_doc_to(doc, &output_path);
    unsafe { CGPDFDocumentRelease(doc) };
    res?;
    Ok(UnlockResult {
        output_path,
        matched_label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Profile {
        Profile {
            name: "Jitesh Dhamaniya".to_string(),
            dob: "1990-02-02".to_string(),
            card_last4: "1234".to_string(),
            account_last4: "5678".to_string(),
            customer_id: "CRN9001".to_string(),
            pan: "ABCDE1234F".to_string(),
            mobile: "9876543210".to_string(),
        }
    }

    #[test]
    fn generates_common_bank_formats() {
        let c = generate_candidates(&sample());
        // first 4 of name in each casing
        assert!(c.contains(&"JITE".to_string()));
        assert!(c.contains(&"jite".to_string()));
        assert!(c.contains(&"Jite".to_string()));
        // ICICI / Axis style: NAME4 + DDMM
        assert!(c.contains(&"JITE0202".to_string()));
        // HDFC style: NAME4 + DDMMYY
        assert!(c.contains(&"JITE020290".to_string()));
        // SBI Card style: NAME4 + DDMMYYYY
        assert!(c.contains(&"JITE02021990".to_string()));
        // name + card / account last 4
        assert!(c.contains(&"JITE1234".to_string()));
        assert!(c.contains(&"JITE5678".to_string()));
        // PAN and PAN + DOB
        assert!(c.contains(&"ABCDE1234F".to_string()));
        assert!(c.contains(&"ABCDE1234F0202".to_string()));
        // customer id + mobile last 4
        assert!(c.contains(&"CRN9001".to_string()));
        assert!(c.contains(&"3210".to_string()));
        // no empties or duplicates
        assert!(!c.iter().any(|s| s.is_empty()));
        let mut sorted = c.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), c.len(), "candidates must be unique");
    }

    #[test]
    fn empty_profile_yields_nothing() {
        assert!(generate_candidates(&Profile::default()).is_empty());
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            unlock_pdf,
            unlock_pdf_auto,
            list_passwords,
            save_password,
            delete_password,
            get_profile,
            save_profile
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

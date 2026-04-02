// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::CString;
use std::path::Path;

use core_foundation::base::TCFType;
use core_foundation::url::CFURL;

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

fn do_unlock_pdf(input_path: &str, password: &str, output_path: &str) -> Result<(), String> {
    let input_url =
        CFURL::from_path(Path::new(input_path), false).ok_or("Failed to create input URL")?;

    let doc = unsafe { CGPDFDocumentCreateWithURL(input_url.as_concrete_TypeRef() as CFURLRef) };
    if doc.is_null() {
        return Err("Failed to open PDF file".to_string());
    }

    // Unlock if encrypted
    let is_encrypted = unsafe { CGPDFDocumentIsEncrypted(doc) };
    if is_encrypted {
        let c_password = CString::new(password).map_err(|_| "Invalid password string")?;
        let unlocked = unsafe { CGPDFDocumentUnlockWithPassword(doc, c_password.as_ptr()) };
        if !unlocked {
            unsafe { CGPDFDocumentRelease(doc) };
            return Err("Wrong password".to_string());
        }
    }

    if !unsafe { CGPDFDocumentIsUnlocked(doc) } {
        unsafe { CGPDFDocumentRelease(doc) };
        return Err("Could not unlock PDF".to_string());
    }

    let page_count = unsafe { CGPDFDocumentGetNumberOfPages(doc) };
    if page_count == 0 {
        unsafe { CGPDFDocumentRelease(doc) };
        return Err("PDF has no pages".to_string());
    }

    // Create output PDF context
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
        unsafe { CGPDFDocumentRelease(doc) };
        return Err("Failed to create output PDF".to_string());
    }

    // Copy each page to the new (unencrypted) PDF
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
        CGPDFDocumentRelease(doc);
    }

    Ok(())
}

#[tauri::command]
fn unlock_pdf(file_path: String, password: String) -> Result<String, String> {
    let input = Path::new(&file_path);
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
    let output_path = format!("{}/{}_unlocked.pdf", parent, stem);

    do_unlock_pdf(&file_path, &password, &output_path)?;
    Ok(output_path)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![unlock_pdf])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

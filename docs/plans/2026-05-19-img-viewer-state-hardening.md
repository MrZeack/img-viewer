# Img Viewer State Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove the highest-risk state panics and make file-move / undo behavior consistent with the displayed image.

**Architecture:** Keep the current module split, but add small guard helpers in `state`/`core`/`input` so navigation and async events never index into an empty image list. Refresh view state after undo and sanitize user-facing strings before rendering them. The first batch should stay narrowly scoped to avoid changing the rendering or loader pipeline.

**Tech Stack:** Rust 2021, `eframe`/`egui`, standard library only for these fixes.

---

### Task 1: Guard current image access

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/app/core.rs`
- Modify: `src/app/ui.rs`

**Step 1: Write the failing test**

Add a unit test for an empty image list helper that returns `None` instead of indexing.

**Step 2: Run the test to verify it fails**

Run: `cargo test`
Expected: the new test fails or does not compile until the helper exists.

**Step 3: Write minimal implementation**

Add a helper on `ImageViewerApp` that returns `Option<&PathBuf>` for the current file and use it everywhere we currently index directly.

**Step 4: Run the test to verify it passes**

Run: `cargo test`
Expected: pass.

### Task 2: Fix undo refresh behavior

**Files:**
- Modify: `src/app/input.rs`

**Step 1: Write the failing test**

Add a focused test for restoring a moved file and ensuring the app reloads the current image state.

**Step 2: Run the test to verify it fails**

Run: `cargo test`
Expected: fails until undo triggers a refresh.

**Step 3: Write minimal implementation**

After a successful undo, reload the restored image and metadata or clear the stale texture before returning.

**Step 4: Run the test to verify it passes**

Run: `cargo test`
Expected: pass.

### Task 3: Make bookmark status text UTF-8 safe

**Files:**
- Modify: `src/app/input.rs`

**Step 1: Write the failing test**

Add a unit test that formats a bookmark label from a multibyte filename and confirms it does not panic.

**Step 2: Run the test to verify it fails**

Run: `cargo test`
Expected: fails until the truncation is character-safe.

**Step 3: Write minimal implementation**

Replace byte slicing with a `chars()`-based truncation helper.

**Step 4: Run the test to verify it passes**

Run: `cargo test`
Expected: pass.


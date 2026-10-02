// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! UniFFI's generator in library mode (the documented setup): reads the
//! exported metadata from the built `cox-ffi` library and writes the Swift
//! bindings. A bin of its own so the generator is never linked into the app.

fn main() {
    uniffi::uniffi_bindgen_main()
}

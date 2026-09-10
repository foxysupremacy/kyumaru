#![allow(non_snake_case, dead_code, unused_imports)]

mod config;
mod hooks;
mod il2cpp;
mod persistence;
mod plugin_api;
mod reflection;
mod sync;

use crate::config::init_config;
use crate::hooks::{
    lz4_decompress_safe_ext_hook, render_kyumaru_section, veteran_hook, ORIG_LOAD_INDEX,
    ORIG_LZ4_DECOMPRESS, ORIG_VETERAN_APPLY,
};
use crate::il2cpp::{find_image_by_name, init_il2cpp_methods, method_addr, RawIl2CppImage};
use crate::plugin_api::{
    api, hook, il2cpp_resolve, init_with_get_api, init_with_vtable_v2, HachimiGetApiFn,
    InitResult, VtableV2,
};
use crate::reflection::find_methods_in_assembly_by_param;

#[cfg(target_os = "windows")]
extern "system" {
    fn GetModuleHandleA(lpModuleName: *const u8) -> *mut std::ffi::c_void;
    fn GetProcAddress(
        hModule: *mut std::ffi::c_void,
        lpProcName: *const u8,
    ) -> *mut std::ffi::c_void;
}

#[cfg(not(target_os = "windows"))]
unsafe fn GetModuleHandleA(_lpModuleName: *const u8) -> *mut std::ffi::c_void {
    std::ptr::null_mut()
}

#[cfg(not(target_os = "windows"))]
unsafe fn GetProcAddress(
    _hModule: *mut std::ffi::c_void,
    _lpProcName: *const u8,
) -> *mut std::ffi::c_void {
    std::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn hachimi_init(vtable_ptr: *const VtableV2, version: i32) -> InitResult {
    if vtable_ptr.is_null() || version < 2 {
        return InitResult::Error;
    }
    init_with_vtable_v2(vtable_ptr);
    init_plugin()
}

#[no_mangle]
pub extern "C" fn hachimi_init_v3(get_api: HachimiGetApiFn, version: i32) -> InitResult {
    if version < 3 {
        return InitResult::Error;
    }
    init_with_get_api(get_api);
    init_plugin()
}

fn init_plugin() -> InitResult {
    log!("Kyumaru {} initialized.", env!("CARGO_PKG_VERSION"));

    if let Err(e) = init_config() {
        log!("Failed to initialize config: {}", e);
        return InitResult::Error;
    }

    unsafe {
        if !init_il2cpp_methods(|name| il2cpp_resolve(name)) {
            log!("[Kyumaru] Warning: IL2CPP scanning functions not resolved (IL2CPP may not be loaded yet).");
        }
    }

    // Register in-game Hachimi overlay menu section
    if let Some(register_section) = api().gui_register_menu_section {
        unsafe {
            register_section(Some(render_kyumaru_section), std::ptr::null_mut());
            log!("[Kyumaru] Hachimi overlay menu section registered.");
        }
    }

    std::thread::spawn(|| unsafe {
        install_hooks();
    });

    InitResult::Ok
}

unsafe fn install_hooks() {
    log!("[Hooks] Starting hook installation...");

    // 1. Hook libnative.dll's LZ4_decompress_safe_ext (CarrotJuicer approach)
    std::thread::spawn(|| {
        log!("[Hooks] Waiting for libnative.dll to load...");
        let mut libnative = std::ptr::null_mut();
        for _ in 0..300 {
            // Poll every 100ms up to 30 seconds
            unsafe {
                libnative = GetModuleHandleA(b"libnative.dll\0".as_ptr());
                if !libnative.is_null() {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if libnative.is_null() {
            log!("[Hooks] Error: libnative.dll not found within 30s timeout.");
            return;
        }

        log!("[Hooks] Found libnative.dll at {:p}", libnative);

        let lz4_ptr = unsafe {
            GetProcAddress(libnative, b"LZ4_decompress_safe_ext\0".as_ptr())
        };

        if lz4_ptr.is_null() {
            log!("[Hooks] Error: LZ4_decompress_safe_ext symbol not found in libnative.dll.");
            return;
        }

        log!("[Hooks] LZ4_decompress_safe_ext at {:p}", lz4_ptr);

        unsafe {
            if let Some(orig) = hook(lz4_ptr as usize, lz4_decompress_safe_ext_hook as *const () as usize) {
                ORIG_LZ4_DECOMPRESS = orig;
                log!("[Hooks] Successfully installed LZ4_decompress_safe_ext hook! Waiting for load/index...");
            } else {
                log!("[Hooks] Error: Failed to install LZ4_decompress_safe_ext hook via Hachimi.");
            }
        }
    });

    // 2. Optionally hook Hall of Fame veterans (WorkTrainedCharaData.UpdateAll) in IL2CPP
    let mut target_image = find_image_by_name("umamusume");
    if target_image.is_null() {
        target_image = find_image_by_name("Assembly-CSharp");
    }
    if target_image.is_null() {
        target_image = find_image_by_name("Gallop");
    }

    if !target_image.is_null() {
        let veteran_results =
            find_methods_in_assembly_by_param(target_image as *mut RawIl2CppImage, "TrainedChara[]");

        let best_vet_candidate = veteran_results
            .iter()
            .find(|r| {
                r.class_name.contains("WorkTrainedCharaData") && r.method_name.contains("UpdateAll")
            })
            .or_else(|| veteran_results.first());

        if let Some(result) = best_vet_candidate {
            let fn_ptr = method_addr(result.method);
            if fn_ptr != 0 {
                if let Some(orig) = hook(fn_ptr, veteran_hook as *const () as usize) {
                    ORIG_VETERAN_APPLY = orig;
                    log!(
                        "[Hooks] Veteran hook installed on {}.{}",
                        result.class_name,
                        result.method_name
                    );
                }
            }
        }
    }
}

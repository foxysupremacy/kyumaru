use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitResult {
    Error = 0,
    Ok = 1,
}

pub type HachimiGetApiFn = unsafe extern "C" fn(*const c_char) -> *mut c_void;

// Function pointer signatures
pub type FnHachimiInstance = unsafe extern "C" fn() -> *const c_void;
pub type FnHachimiGetInterceptor = unsafe extern "C" fn(*const c_void) -> *const c_void;
pub type FnInterceptorHook =
    unsafe extern "C" fn(*const c_void, *mut c_void, *mut c_void) -> *mut c_void;
pub type FnIl2cppResolveSymbol = unsafe extern "C" fn(*const c_char) -> *mut c_void;
pub type FnLog = unsafe extern "C" fn(i32, *const c_char, *const c_char);
pub type FnGuiRegisterMenuSection =
    unsafe extern "C" fn(Option<extern "C" fn(*mut c_void, *mut c_void)>, *mut c_void) -> bool;
pub type FnGuiShowNotification = unsafe extern "C" fn(*const c_char) -> bool;
pub type FnGuiUiHeading = unsafe extern "C" fn(*mut c_void, *const c_char) -> bool;
pub type FnGuiUiLabel = unsafe extern "C" fn(*mut c_void, *const c_char) -> bool;
pub type FnGuiUiSmall = unsafe extern "C" fn(*mut c_void, *const c_char) -> bool;
pub type FnGuiUiSeparator = unsafe extern "C" fn(*mut c_void) -> bool;
pub type FnGuiUiButton = unsafe extern "C" fn(*mut c_void, *const c_char) -> bool;
pub type FnGuiUiColoredLabel =
    unsafe extern "C" fn(*mut c_void, u8, u8, u8, u8, *const c_char) -> bool;

#[derive(Default)]
pub struct ApiTable {
    pub hachimi_instance: Option<FnHachimiInstance>,
    pub hachimi_get_interceptor: Option<FnHachimiGetInterceptor>,
    pub interceptor_hook: Option<FnInterceptorHook>,
    pub il2cpp_resolve_symbol: Option<FnIl2cppResolveSymbol>,
    pub log: Option<FnLog>,
    pub gui_register_menu_section: Option<FnGuiRegisterMenuSection>,
    pub gui_show_notification: Option<FnGuiShowNotification>,
    pub gui_ui_heading: Option<FnGuiUiHeading>,
    pub gui_ui_label: Option<FnGuiUiLabel>,
    pub gui_ui_small: Option<FnGuiUiSmall>,
    pub gui_ui_separator: Option<FnGuiUiSeparator>,
    pub gui_ui_button: Option<FnGuiUiButton>,
    pub gui_ui_colored_label: Option<FnGuiUiColoredLabel>,
}

static API: OnceLock<ApiTable> = OnceLock::new();

pub fn init_with_get_api(get_api: HachimiGetApiFn) {
    unsafe {
        let resolve = |name: &str| -> *mut c_void {
            let cstr = CString::new(name).unwrap();
            get_api(cstr.as_ptr())
        };

        macro_rules! load {
            ($name:ident) => {
                std::mem::transmute(resolve(stringify!($name)))
            };
        }

        let table = ApiTable {
            hachimi_instance: load!(hachimi_instance),
            hachimi_get_interceptor: load!(hachimi_get_interceptor),
            interceptor_hook: load!(interceptor_hook),
            il2cpp_resolve_symbol: load!(il2cpp_resolve_symbol),
            log: load!(log),
            gui_register_menu_section: load!(gui_register_menu_section),
            gui_show_notification: load!(gui_show_notification),
            gui_ui_heading: load!(gui_ui_heading),
            gui_ui_label: load!(gui_ui_label),
            gui_ui_small: load!(gui_ui_small),
            gui_ui_separator: load!(gui_ui_separator),
            gui_ui_button: load!(gui_ui_button),
            gui_ui_colored_label: load!(gui_ui_colored_label),
        };
        let _ = API.set(table);
    }
}

// Fallback Vtable layout for Hachimi v2
#[repr(C)]
pub struct VtableV2 {
    pub hachimi_instance: Option<FnHachimiInstance>,
    pub hachimi_get_interceptor: Option<FnHachimiGetInterceptor>,
    pub interceptor_hook: Option<FnInterceptorHook>,
    pub _pad1: [usize; 3], // interceptor_hook_vtable, trampoline, unhook
    pub il2cpp_resolve_symbol: Option<FnIl2cppResolveSymbol>,
}

pub fn init_with_vtable_v2(vt: *const VtableV2) {
    unsafe {
        if vt.is_null() {
            return;
        }
        let v = &*vt;
        let table = ApiTable {
            hachimi_instance: v.hachimi_instance,
            hachimi_get_interceptor: v.hachimi_get_interceptor,
            interceptor_hook: v.interceptor_hook,
            il2cpp_resolve_symbol: v.il2cpp_resolve_symbol,
            ..Default::default()
        };
        let _ = API.set(table);
    }
}

pub fn api() -> &'static ApiTable {
    API.get().expect("Kyumaru API table not initialized")
}

pub unsafe fn hook(fn_ptr: usize, hook_fn: usize) -> Option<usize> {
    if fn_ptr == 0 {
        return None;
    }
    let a = api();
    let get_inst = a.hachimi_instance?;
    let get_inter = a.hachimi_get_interceptor?;
    let do_hook = a.interceptor_hook?;

    let hachimi = get_inst();
    let interceptor = get_inter(hachimi);
    let orig = do_hook(interceptor, fn_ptr as *mut c_void, hook_fn as *mut c_void);
    if orig.is_null() {
        None
    } else {
        Some(orig as usize)
    }
}

pub unsafe fn il2cpp_resolve(name: &CStr) -> *mut c_void {
    if let Some(res) = api().il2cpp_resolve_symbol {
        res(name.as_ptr())
    } else {
        std::ptr::null_mut()
    }
}

pub unsafe fn show_toast(msg: &str) {
    if let Some(notify) = api().gui_show_notification {
        if let Ok(cstr) = CString::new(msg) {
            notify(cstr.as_ptr());
        }
    }
}

pub unsafe fn ui_heading(ui: *mut c_void, text: &str) {
    if let Some(f) = api().gui_ui_heading {
        if let Ok(c) = CString::new(text) {
            f(ui, c.as_ptr());
        }
    }
}

pub unsafe fn ui_label(ui: *mut c_void, text: &str) {
    if let Some(f) = api().gui_ui_label {
        if let Ok(c) = CString::new(text) {
            f(ui, c.as_ptr());
        }
    }
}

pub unsafe fn ui_small(ui: *mut c_void, text: &str) {
    if let Some(f) = api().gui_ui_small {
        if let Ok(c) = CString::new(text) {
            f(ui, c.as_ptr());
        }
    }
}

pub unsafe fn ui_separator(ui: *mut c_void) {
    if let Some(f) = api().gui_ui_separator {
        f(ui);
    }
}

pub unsafe fn ui_button(ui: *mut c_void, text: &str) -> bool {
    if let Some(f) = api().gui_ui_button {
        if let Ok(c) = CString::new(text) {
            return f(ui, c.as_ptr());
        }
    }
    false
}

pub unsafe fn ui_colored_label(ui: *mut c_void, r: u8, g: u8, b: u8, a: u8, text: &str) {
    if let Some(f) = api().gui_ui_colored_label {
        if let Ok(c) = CString::new(text) {
            f(ui, r, g, b, a, c.as_ptr());
        }
    }
}

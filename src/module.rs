use llvm_sys::analysis::{LLVMVerifyModule, LLVMVerifierFailureAction};
use llvm_sys::bit_writer::{LLVMWriteBitcodeToFile, LLVMWriteBitcodeToMemoryBuffer, LLVMWriteBitcodeToFD};
use llvm_sys::core::{LLVMAddFunction, LLVMAddGlobal, LLVMCreateFunctionPassManagerForModule, LLVMDisposeMessage, LLVMDumpModule, LLVMGetNamedFunction, LLVMGetTypeByName, LLVMSetDataLayout, LLVMSetInitializer, LLVMSetTarget, LLVMCloneModule, LLVMDisposeModule, LLVMGetTarget, LLVMGetDataLayout, LLVMModuleCreateWithName, LLVMGetModuleContext, LLVMGetFirstFunction, LLVMGetLastFunction, LLVMSetLinkage, LLVMAddGlobalInAddressSpace};
use llvm_sys::execution_engine::{LLVMCreateExecutionEngineForModule, LLVMLinkInInterpreter, LLVMLinkInMCJIT};
use llvm_sys::prelude::LLVMModuleRef;
use llvm_sys::LLVMLinkage;

use std::ffi::{CString, CStr};
use std::fs::File;
use std::mem::{uninitialized, zeroed};
use std::path::Path;
use std::os::unix::io::AsRawFd;

use context::{Context, ContextRef};
use data_layout::DataLayout;
use execution_engine::ExecutionEngine;
use memory_buffer::MemoryBuffer;
use pass_manager::PassManager;
use types::{AsTypeRef, BasicType, FunctionType, BasicTypeEnum};
use values::{BasicValue, FunctionValue, PointerValue};

// REVIEW: Maybe this should go into it's own module?
#[derive(Debug, PartialEq)]
pub enum Linkage {
    AppendingLinkage,
    AvailableExternallyLinkage,
    CommonLinkage,
    DLLExportLinkage,
    DLLImportLinkage,
    ExternalLinkage,
    ExternalWeakLinkage,
    GhostLinkage,
    InternalLinkage,
    LinkerPrivateLinkage,
    LinkerPrivateWeakLinkage,
    LinkOnceAnyLinkage,
    LinkOnceODRAutoHideLinkage,
    LinkOnceODRLinkage,
    PrivateLinkage,
    WeakAnyLinkage,
    WeakODRLinkage,
}

impl Linkage {
    pub(crate) fn new(linkage: LLVMLinkage) -> Self {
        match linkage {
            LLVMLinkage::LLVMAppendingLinkage => Linkage::AppendingLinkage,
            LLVMLinkage::LLVMAvailableExternallyLinkage => Linkage::AvailableExternallyLinkage,
            LLVMLinkage::LLVMCommonLinkage => Linkage::CommonLinkage,
            LLVMLinkage::LLVMDLLExportLinkage => Linkage::DLLExportLinkage,
            LLVMLinkage::LLVMDLLImportLinkage => Linkage::DLLImportLinkage,
            LLVMLinkage::LLVMExternalLinkage => Linkage::ExternalLinkage,
            LLVMLinkage::LLVMExternalWeakLinkage => Linkage::ExternalWeakLinkage,
            LLVMLinkage::LLVMGhostLinkage => Linkage::GhostLinkage,
            LLVMLinkage::LLVMInternalLinkage => Linkage::InternalLinkage,
            LLVMLinkage::LLVMLinkerPrivateLinkage => Linkage::LinkerPrivateLinkage,
            LLVMLinkage::LLVMLinkerPrivateWeakLinkage => Linkage::LinkerPrivateWeakLinkage,
            LLVMLinkage::LLVMLinkOnceAnyLinkage => Linkage::LinkOnceAnyLinkage,
            LLVMLinkage::LLVMLinkOnceODRAutoHideLinkage => Linkage::LinkOnceODRAutoHideLinkage,
            LLVMLinkage::LLVMLinkOnceODRLinkage => Linkage::LinkOnceODRLinkage,
            LLVMLinkage::LLVMPrivateLinkage => Linkage::PrivateLinkage,
            LLVMLinkage::LLVMWeakAnyLinkage => Linkage::WeakAnyLinkage,
            LLVMLinkage::LLVMWeakODRLinkage => Linkage::WeakODRLinkage,
        }
    }

    fn as_llvm_linkage(&self) -> LLVMLinkage {
        match *self {
            Linkage::AppendingLinkage => LLVMLinkage::LLVMAppendingLinkage,
            Linkage::AvailableExternallyLinkage => LLVMLinkage::LLVMAvailableExternallyLinkage,
            Linkage::CommonLinkage => LLVMLinkage::LLVMCommonLinkage,
            Linkage::DLLExportLinkage => LLVMLinkage::LLVMDLLExportLinkage,
            Linkage::DLLImportLinkage => LLVMLinkage::LLVMDLLImportLinkage,
            Linkage::ExternalLinkage => LLVMLinkage::LLVMExternalLinkage,
            Linkage::ExternalWeakLinkage => LLVMLinkage::LLVMExternalWeakLinkage,
            Linkage::GhostLinkage => LLVMLinkage::LLVMGhostLinkage,
            Linkage::InternalLinkage => LLVMLinkage::LLVMInternalLinkage,
            Linkage::LinkerPrivateLinkage => LLVMLinkage::LLVMLinkerPrivateLinkage,
            Linkage::LinkerPrivateWeakLinkage => LLVMLinkage::LLVMLinkerPrivateWeakLinkage,
            Linkage::LinkOnceAnyLinkage => LLVMLinkage::LLVMLinkOnceAnyLinkage,
            Linkage::LinkOnceODRAutoHideLinkage => LLVMLinkage::LLVMLinkOnceODRAutoHideLinkage,
            Linkage::LinkOnceODRLinkage => LLVMLinkage::LLVMLinkOnceODRLinkage,
            Linkage::PrivateLinkage => LLVMLinkage::LLVMPrivateLinkage,
            Linkage::WeakAnyLinkage => LLVMLinkage::LLVMWeakAnyLinkage,
            Linkage::WeakODRLinkage => LLVMLinkage::LLVMWeakODRLinkage,
        }
    }
}

pub struct Module {
    pub(crate) module: LLVMModuleRef,
}

impl Module {
    pub(crate) fn new(module: LLVMModuleRef) -> Self {
        assert!(!module.is_null());

        Module {
            module: module
        }
    }

    pub fn create(name: &str) -> Self {
        let c_string = CString::new(name).expect("Conversion to CString failed unexpectedly");

        let module = unsafe {
            LLVMModuleCreateWithName(c_string.as_ptr())
        };

        Module::new(module)
    }

    // TODO: Worth documenting that LLVM will default linkage to ExternalLinkage (at least in 3.7)
    pub fn add_function(&self, name: &str, return_type: &FunctionType, linkage: Option<&Linkage>) -> FunctionValue {
        let c_string = CString::new(name).expect("Conversion to CString failed unexpectedly");

        let value = unsafe {
            LLVMAddFunction(self.module, c_string.as_ptr(), return_type.as_type_ref())
        };

        if let Some(linkage) = linkage {
            unsafe {
                LLVMSetLinkage(value, linkage.as_llvm_linkage());
            }
        }

        FunctionValue::new(value)
    }

    pub fn get_context(&self) -> ContextRef {
        let context = unsafe {
            LLVMGetModuleContext(self.module)
        };

        ContextRef::new(Context::new(context))
    }

    pub fn get_first_function(&self) -> Option<FunctionValue> {
        let function = unsafe {
            LLVMGetFirstFunction(self.module)
        };

        if function.is_null() {
            return None;
        }

        Some(FunctionValue::new(function))
    }

    pub fn get_last_function(&self) -> Option<FunctionValue> {
        let function = unsafe {
            LLVMGetLastFunction(self.module)
        };

        if function.is_null() {
            return None;
        }

        Some(FunctionValue::new(function))
    }

    pub fn get_function(&self, name: &str) -> Option<FunctionValue> {
        let c_string = CString::new(name).expect("Conversion to CString failed unexpectedly");

        let value = unsafe {
            LLVMGetNamedFunction(self.module, c_string.as_ptr())
        };

        if value.is_null() {
            return None;
        }

        Some(FunctionValue::new(value))
    }

    pub fn get_type(&self, name: &str) -> Option<BasicTypeEnum> {
        let c_string = CString::new(name).expect("Conversion to CString failed unexpectedly");

        let type_ = unsafe {
            LLVMGetTypeByName(self.module, c_string.as_ptr())
        };

        if type_.is_null() {
            return None;
        }

        Some(BasicTypeEnum::new(type_))
    }

    // TODO: Make this take a targets::Target object by ref and call get_name
    pub fn set_target(&self, target_triple: &str) {
        let c_string = CString::new(target_triple).expect("Conversion to CString failed unexpectedly");

        unsafe {
            LLVMSetTarget(self.module, c_string.as_ptr())
        }
    }

    pub fn get_target(&self) -> &CStr {
        unsafe {
            CStr::from_ptr(LLVMGetTarget(self.module))
        }
    }

    pub fn create_execution_engine(&self, jit_mode: bool) -> Result<ExecutionEngine, String> {
        let mut execution_engine = unsafe { uninitialized() };
        let mut err_str = unsafe { zeroed() };

        if jit_mode {
            unsafe {
                LLVMLinkInMCJIT();
            }
        }

        unsafe {
            LLVMLinkInInterpreter();
        }

        let code = unsafe {
            LLVMCreateExecutionEngineForModule(&mut execution_engine, self.module, &mut err_str) // Should take ownership of module
        };

        if code == 1 {
            let rust_str = unsafe {
                let rust_str = CStr::from_ptr(err_str).to_string_lossy().into_owned();

                LLVMDisposeMessage(err_str);

                rust_str
            };

            return Err(rust_str);
        }

        Ok(ExecutionEngine::new(execution_engine, jit_mode))
    }

    pub fn create_function_pass_manager(&self) -> PassManager {
        let pass_manager = unsafe {
            LLVMCreateFunctionPassManagerForModule(self.module)
        };

        PassManager::new(pass_manager)
    }

    // REVIEW: Is this really always a pointer? It would make sense...
    pub fn add_global(&self, type_: &BasicType, initial_value: Option<&BasicValue>, address_space: Option<u32>, name: &str) -> PointerValue {
        let c_string = CString::new(name).expect("Conversion to CString failed unexpectedly");

        let value = unsafe {
            match address_space {
                Some(address_space) => LLVMAddGlobalInAddressSpace(self.module, type_.as_type_ref(), c_string.as_ptr(), address_space),
                None => LLVMAddGlobal(self.module, type_.as_type_ref(), c_string.as_ptr()),
            }
        };

        if let Some(init_val) = initial_value {
            unsafe {
                LLVMSetInitializer(value, init_val.as_value_ref())
            }
        }

        PointerValue::new(value)
    }

    pub fn write_bitcode_to_path(&self, path: &Path) -> bool {
        let path_str = path.to_str().expect("Did not find a valid Unicode path string");
        let c_string = CString::new(path_str).expect("Conversion to CString failed unexpectedly");

        unsafe {
            LLVMWriteBitcodeToFile(self.module, c_string.as_ptr()) == 0
        }
    }

    // See GH issue #6
    fn write_bitcode_to_file(&self, file: &File, should_close: bool, unbuffered: bool) -> bool {
        // REVIEW: as_raw_fd docs suggest it only works in *nix
        // Also, should_close should maybe be hardcoded to true?
        unsafe {
            LLVMWriteBitcodeToFD(self.module, file.as_raw_fd(), should_close as i32, unbuffered as i32) == 0
        }
    }

    // REVIEW: Untested
    pub fn write_bitcode_to_memory(&self) -> MemoryBuffer {
        let memory_buffer = unsafe {
            LLVMWriteBitcodeToMemoryBuffer(self.module)
        };

        MemoryBuffer::new(memory_buffer)
    }

    pub fn verify(&self, print: bool) -> bool {
        let err_str: *mut *mut i8 = unsafe { zeroed() };

        let action = if print {
            LLVMVerifierFailureAction::LLVMPrintMessageAction
        } else {
            LLVMVerifierFailureAction::LLVMReturnStatusAction
        };

        let code = unsafe {
            LLVMVerifyModule(self.module, action, err_str)
        };

        if code == 1 && !err_str.is_null() {
            unsafe {
                if print {
                    let rust_str = CStr::from_ptr(*err_str).to_str().unwrap();

                    println!("{}", rust_str); // FIXME: Should probably be stderr?
                }

                LLVMDisposeMessage(*err_str);
            }
        }

        code == 0
    }

    pub fn get_data_layout(&self) -> &CStr {
        unsafe {
            CStr::from_ptr(LLVMGetDataLayout(self.module))
        }
    }

    pub fn set_data_layout(&self, data_layout: DataLayout) {
        unsafe {
            LLVMSetDataLayout(self.module, data_layout.data_layout)
        }
    }

    pub fn print_to_stderr(&self) {
        unsafe {
            LLVMDumpModule(self.module);
        }
    }
}

impl Clone for Module {
    fn clone(&self) -> Self {
        let module = unsafe {
            LLVMCloneModule(self.module)
        };

        Module::new(module)
    }
}

// FIXME: Causes segfault in test(s)
// impl Drop for Module {
//     fn drop(&mut self) {
//         unsafe {
//             LLVMDisposeModule(self.module)
//         }
//     }
// }

#[test]
fn test_write_bitcode_to_path() {
    use context::Context;
    use std::env::temp_dir;
    use std::fs::{File, remove_file};
    use std::io::Read;

    let mut path = temp_dir();

    path.push("temp.bc");

    let context = Context::create();
    let module = context.create_module("my_module");
    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);

    module.add_function("my_fn", &fn_type, None);
    module.write_bitcode_to_path(&path);

    let mut contents = Vec::new();
    let mut file = File::open(&path).expect("Could not open temp file");

    file.read_to_end(&mut contents).expect("Unable to verify written file");

    assert!(contents.len() > 0);

    remove_file(&path).unwrap();
}

// REVIEW: This test infrequently fails. Seems to happen more often on travis.
// Possibly a LLVM bug? Wrapper is really straightforward. See issue #6 on GH
// #[test]
// fn test_write_bitcode_to_file() {
//     use context::Context;
//     use std::env::temp_dir;
//     use std::fs::{File, remove_file};
//     use std::io::{Read, Seek, SeekFrom};

//     let mut path = temp_dir();

//     path.push("temp2.bc");

//     let mut file = File::create(&path).unwrap();

//     let context = Context::create();
//     let module = context.create_module("my_module");
//     let void_type = context.void_type();
//     let fn_type = void_type.fn_type(&[], false);

//     module.add_function("my_fn", &fn_type, None);
//     module.write_bitcode_to_file(&file, true, false);

//     let mut contents = Vec::new();
//     let mut file2 = File::open(&path).expect("Could not open temp file");

//     file.read_to_end(&mut contents).expect("Unable to verify written file");

//     assert!(contents.len() > 0);

//     remove_file(&path).unwrap();
// }

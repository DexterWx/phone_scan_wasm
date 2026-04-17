/**
 * 构建小程序专用 WASM 包
 *
 * 用法: node scripts/build_miniprogram.js
 *
 * 基于 wasm-pack build --target web 的产物，自动转换为小程序兼容格式：
 * 1. ES Module → CommonJS
 * 2. WebAssembly → WXWebAssembly
 * 3. 移除 import.meta.url、fetch 等小程序不支持的 API
 * 4. 消除 typeof 表达式（避免 Babel 转译引入 @babel/runtime 依赖）
 * 5. 移除 externref + multivalue（iOS WXWebAssembly 不支持）
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PKG_DIR = path.join(__dirname, '..', 'pkg');
const OUT_DIR = path.join(__dirname, '..', 'pkg_miniprogram');

// ============================================================
// Part 1: JS 转换
// ============================================================

// 读取原始文件
const srcJs = fs.readFileSync(path.join(PKG_DIR, 'phone_scan_wasm.js'), 'utf-8');

// 转换 JS
let output = srcJs;

// 1. 移除顶部的 ts-self-types 注释
output = output.replace(/\/\* @ts-self-types.*?\*\/\n*/g, '');

// 2. 移除所有 export 关键字（包括 export default）
output = output.replace(/^export default /gm, '');
output = output.replace(/^export /gm, '');

// 3. 移除残留的 ES Module 导出语句
//    如 "{ initSync };"  /  "{ initSync, __wbg_init as default };"
output = output.replace(/^\{\s*initSync\s*(?:,\s*__wbg_init\s+as\s+default\s*)?\}\s*;?\s*$/gm, '');
output = output.replace(/^__wbg_init\s*;?\s*$/gm, '');

// 4. 替换 WebAssembly → WXWebAssembly
output = output.replace(/\bWebAssembly\b/g, 'WXWebAssembly');

// 5. 消除所有 typeof 表达式（小程序 Babel 会把 typeof 转成 require('@babel/runtime/helpers/typeof')）

// 5a. TextDecoder/TextEncoder 在小程序环境一定存在，直接替换初始化
output = output.replace(
    /const cachedTextDecoder = \(typeof TextDecoder[\s\S]*?\);/,
    "const cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });"
);
output = output.replace(
    /if \(typeof TextDecoder !== 'undefined'\) \{\s*cachedTextDecoder\.decode\(\)\s*\}\s*;?/,
    'cachedTextDecoder.decode();'
);
// 也处理 typeof 已被替换后的版本
output = output.replace(
    /if \(\(TextDecoder !== void 0\)\) \{\s*cachedTextDecoder\.decode\(\)\s*\}\s*;?/,
    'cachedTextDecoder.decode();'
);
output = output.replace(
    /const cachedTextEncoder = \(typeof TextEncoder[\s\S]*?\);/,
    "const cachedTextEncoder = new TextEncoder('utf-8');"
);

// 5b. encodeString 整块替换（含 typeof 检查和多行三元表达式）
output = output.replace(
    /const encodeString = \(typeof cachedTextEncoder\.encodeInto[\s\S]*?^\}\);/m,
    `const encodeString = cachedTextEncoder.encodeInto
    ? function (arg, view) { return cachedTextEncoder.encodeInto(arg, view); }
    : function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return { read: arg.length, written: buf.length };
    };`
);

// 5c. FinalizationRegistry（可能不存在于某些小程序环境）
output = output.replace(
    /\(typeof FinalizationRegistry === 'undefined'\)/g,
    '(!globalThis.FinalizationRegistry)'
);

// 5d. initSync 中的 typeof module !== 'undefined'
output = output.replace(
    /typeof module !== 'undefined'/g,
    'module !== void 0'
);

// 5e. __wbg_init 中的 typeof 检查
output = output.replace(/typeof module_or_path !== 'undefined'/g, 'module_or_path !== void 0');
output = output.replace(/typeof module_or_path === 'undefined'/g, 'module_or_path === void 0');
output = output.replace(/typeof module_or_path === 'string'/g, "module_or_path != null && module_or_path.constructor === String");

// 5f. 兜底：替换所有剩余的 typeof X === 'Y' / typeof X !== 'Y' 模式
output = output.replace(/typeof\s+(\w+)\s*===\s*'undefined'/g, '($1 === void 0)');
output = output.replace(/typeof\s+(\w+)\s*!==\s*'undefined'/g, '($1 !== void 0)');
output = output.replace(/typeof\s+(\w+)\s*===\s*'function'/g, '($1 != null && $1.call !== void 0)');
output = output.replace(/typeof\s+(\w+)\s*===\s*'string'/g, '($1 != null && $1.constructor === String)');

// 6. 替换 __wbg_init：移除 import.meta.url / fetch / Response / URL
const initFuncRe = /async function __wbg_init\(module_or_path\)[\s\S]*?^}/m;
output = output.replace(initFuncRe, `async function __wbg_init(module_or_path) {
    if (wasm !== void 0) return wasm;

    if (module_or_path !== void 0) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        }
    }

    if (module_or_path === void 0) {
        throw new Error('小程序环境必须传入 wasm 路径或 ArrayBuffer');
    }

    const imports = __wbg_get_imports();

    // 小程序环境：支持路径字符串或 ArrayBuffer
    if (module_or_path != null && module_or_path.constructor === String) {
        const { instance, module } = await WXWebAssembly.instantiate(module_or_path, imports);
        return __wbg_finalize_init(instance, module);
    } else {
        const { instance, module } = await __wbg_load(module_or_path, imports);
        return __wbg_finalize_init(instance, module);
    }
}`);

// 7. 替换 __wbg_load：移除 Response / fetch 相关逻辑
const loadFuncRe = /async function __wbg_load\(module, imports\)[\s\S]*?^}/m;
output = output.replace(loadFuncRe, `async function __wbg_load(module, imports) {
    const instance = await WXWebAssembly.instantiate(module, imports);
    if (instance instanceof WXWebAssembly.Instance) {
        return { instance, module };
    } else {
        return instance;
    }
}`);

// 7b. 移除 JS 中的 __wbindgen_init_externref_table（对应 wasm 后处理）
output = output.replace(
    /\s*imports\.wbg\.__wbindgen_init_externref_table = function\(\)\s*\{[\s\S]*?\};\s*/,
    '\n'
);

// 7c. 移除 wasm.__wbindgen_start() 调用（它调用的就是 init_externref_table）
output = output.replace(/\s*wasm\.__wbindgen_start\(\);\s*/g, '\n');

// ============================================================
// 8. multivalue → retptr 调用模式转换
//    wasm-pack 生成的 JS 使用 multivalue 返回: ret = wasm.func(args); ret[0], ret[1]
//    移除 multivalue 后，原始函数使用 retptr 模式: wasm.func(retptr, args); 从内存读取
// ============================================================

// 8a. 替换 init_engine 函数体
output = output.replace(
    /function init_engine\(scan_json\)\s*\{[\s\S]*?^\}/m,
    `function init_engine(scan_json) {
    let deferred2_0;
    let deferred2_1;
    const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
    try {
        const ptr0 = passStringToWasm0(scan_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.init_engine(retptr, ptr0, len0);
        var r0 = new DataView(wasm.memory.buffer).getInt32(retptr, true);
        var r1 = new DataView(wasm.memory.buffer).getInt32(retptr + 4, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}`
);

// 8b. 替换 wasminferenceresult_json getter
output = output.replace(
    /get json\(\)\s*\{[\s\S]*?^\s{4}\}/m,
    `get json() {
        let deferred1_0;
        let deferred1_1;
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        try {
            wasm.wasminferenceresult_json(retptr, this.__wbg_ptr);
            var r0 = new DataView(wasm.memory.buffer).getInt32(retptr, true);
            var r1 = new DataView(wasm.memory.buffer).getInt32(retptr + 4, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }`
);

// 8c. 替换 wasminferenceresult_image_data getter
output = output.replace(
    /get image_data\(\)\s*\{[\s\S]*?^\s{4}\}/m,
    `get image_data() {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        try {
            wasm.wasminferenceresult_image_data(retptr, this.__wbg_ptr);
            var r0 = new DataView(wasm.memory.buffer).getInt32(retptr, true);
            var r1 = new DataView(wasm.memory.buffer).getInt32(retptr + 4, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_free(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }`
);

// 8. 添加 CommonJS 导出（先清理可能残留的尾部空行）
output = output.trimEnd() + '\n';

// 8.5 修复 passArray8ToWasm0 — WXWebAssembly memory grow 后需要强制刷新缓存
output = output.replace(
    /function passArray8ToWasm0\(arg, malloc\)\s*\{[\s\S]*?^\}/m,
    `function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    // 强制刷新缓存（malloc 可能触发 memory grow）
    cachedUint8ArrayMemory0 = null;
    const mem = getUint8ArrayMemory0();
    // 检查边界
    if (ptr + arg.length > mem.length) {
        throw new Error('passArray8ToWasm0: 内存越界 ptr=' + ptr + ' len=' + arg.length + ' memSize=' + mem.length);
    }
    mem.set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}`
);

output += `
// CommonJS 导出（小程序兼容）
module.exports = {
    default: __wbg_init,
    initSync,
    init_engine,
    inference_from_rgba,
    WasmInferenceResult,
};
`;

// 10. 最终检查：确保没有残留的 typeof
const remainingTypeof = output.match(/\btypeof\b/g);
if (remainingTypeof) {
    console.warn(`⚠ 警告: 输出中仍有 ${remainingTypeof.length} 处 typeof，可能触发 Babel 转译问题`);
    output.split('\n').forEach((line, i) => {
        if (/\btypeof\b/.test(line)) {
            console.warn(`  行 ${i + 1}: ${line.trim()}`);
        }
    });
}

// 输出 JS
if (!fs.existsSync(OUT_DIR)) {
    fs.mkdirSync(OUT_DIR, { recursive: true });
}

fs.writeFileSync(path.join(OUT_DIR, 'phone_scan_wasm.js'), output);
console.log('✓ JS 转换完成');

// ============================================================
// Part 2: WASM 后处理 — 移除 externref + multivalue（iOS 兼容）
// ============================================================

const wasmSrc = path.join(PKG_DIR, 'phone_scan_wasm_bg.wasm');
const wasmDst = path.join(OUT_DIR, 'phone_scan_wasm_bg.wasm');
const watTmp = path.join(OUT_DIR, '_tmp.wat');

try {
    // 1. wasm → wat
    execSync(`wasm-tools print "${wasmSrc}" -o "${watTmp}"`, { stdio: 'pipe', maxBuffer: 50 * 1024 * 1024 });
    let wat = fs.readFileSync(watTmp, 'utf-8');
    const lines = wat.split('\n');
    const outLines = [];

    let skipFunc = false; // 正在跳过 multivalue shim 函数
    let funcDepth = 0;
    let addedStackPtrExport = false;

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const trimmed = line.trim();

        // 跳过 multivalue shim 函数体
        if (skipFunc) {
            if (trimmed.startsWith('(func ')) funcDepth++;
            if (trimmed === ')') {
                funcDepth--;
                if (funcDepth <= 0) skipFunc = false;
            }
            continue;
        }

        // --- externref 处理 ---

        // 移除 externref table import
        if (trimmed.includes('__wbindgen_init_externref_table')) continue;

        // 完全移除 externref table（table 1）— iOS 不支持多 table
        // 原始: (table (;1;) 128 externref)
        if (/\(table\s+\(;\d+;\)\s+\d+\s+externref\)/.test(trimmed)) continue;

        // 移除对应的 elem 段（声明式，初始化 table 1）
        // (elem (;1;) (i32.const 221) funcref) — 这是空的声明式 elem
        if (/\(elem\s+\(;\d+;\)\s+\(i32\.const\s+\d+\)\s+funcref\)/.test(trimmed)) continue;

        // 移除 externref table 的 export
        if (trimmed.includes('"__wbindgen_export_0"') && trimmed.includes('(table')) continue;
        if (trimmed.includes('"__wbindgen_externrefs"') && trimmed.includes('(export')) continue;

        // 移除 __wbindgen_start export（它调用 init_externref_table）
        if (trimmed.includes('"__wbindgen_start"') && trimmed.includes('(export')) continue;

        // --- multivalue 处理 ---

        // 替换 multivalue type 定义：(result i32 i32) → (result i32)
        // 只改 type 定义行（不是 func 签名行），避免误改
        if (trimmed.startsWith('(type ') && /\(result i32 i32\)/.test(trimmed)) {
            outLines.push(line.replace('(result i32 i32)', '(result i32)'));
            continue;
        }

        // 替换 multivalue shim export → 指向原始函数
        // (export "init_engine" (func $"init_engine multivalue shim"))
        const mvExportMatch = trimmed.match(/\(export "(\w+)" \(func \$"(.+) multivalue shim"\)\)/);
        if (mvExportMatch) {
            const origFunc = mvExportMatch[2];
            outLines.push(line.replace(`$"${origFunc} multivalue shim"`, `$${origFunc}`));
            continue;
        }

        // 跳过 multivalue shim 函数定义
        if (/\(func \$".*multivalue shim"/.test(trimmed)) {
            skipFunc = true;
            funcDepth = 1;
            continue;
        }

        // --- 移除 target_features 自定义段 ---
        if (trimmed.startsWith('(@custom "target_features"')) continue;

        // 在 export 区域后添加 __wbindgen_add_to_stack_pointer export + function
        // 我们在遇到 __wbindgen_realloc export 之后插入
        if (!addedStackPtrExport && trimmed.includes('"__wbindgen_realloc"') && trimmed.includes('(export')) {
            outLines.push(line);
            outLines.push('  (export "__wbindgen_add_to_stack_pointer" (func $__wbindgen_add_to_stack_pointer))');
            addedStackPtrExport = true;
            continue;
        }

        outLines.push(line);
    }

    // 在模块末尾（最后的 ) 之前）插入 __wbindgen_add_to_stack_pointer 函数定义
    // 找到最后一个 ) 并在其前面插入
    const lastCloseParen = outLines.lastIndexOf(')');
    if (lastCloseParen >= 0) {
        outLines.splice(lastCloseParen, 0,
            '  (func $__wbindgen_add_to_stack_pointer (param i32) (result i32)',
            '    global.get $__stack_pointer',
            '    local.get 0',
            '    i32.add',
            '    global.set $__stack_pointer',
            '    global.get $__stack_pointer',
            '  )'
        );
    }

    wat = outLines.join('\n');
    fs.writeFileSync(watTmp, wat);

    // 2. wat → wasm
    execSync(`wasm-tools parse "${watTmp}" -o "${wasmDst}"`, { stdio: 'pipe', maxBuffer: 50 * 1024 * 1024 });
    fs.unlinkSync(watTmp);

    // 3. 验证（输出到临时文件避免 ENOBUFS）
    const verifyTmp = path.join(OUT_DIR, '_verify.wat');
    execSync(`wasm-tools print "${wasmDst}" -o "${verifyTmp}"`, { stdio: 'pipe', maxBuffer: 50 * 1024 * 1024 });
    const checkOutput = fs.readFileSync(verifyTmp, 'utf-8');
    fs.unlinkSync(verifyTmp);

    const externrefCount = (checkOutput.match(/\bexternref\b/g) || []).length;
    const multivalueCount = (checkOutput.match(/\(result\s+i32\s+i32\)/g) || []).length;
    const targetFeaturesCount = (checkOutput.match(/target_features/g) || []).length;
    const hasStackPtrExport = checkOutput.includes('__wbindgen_add_to_stack_pointer');

    if (externrefCount > 0) {
        console.warn(`⚠ 警告: wasm 中仍有 ${externrefCount} 处 externref`);
    }
    if (multivalueCount > 0) {
        console.warn(`⚠ 警告: wasm 中仍有 ${multivalueCount} 处 multivalue (result i32 i32)`);
    }
    if (targetFeaturesCount > 0) {
        console.warn(`⚠ 警告: wasm 中仍有 target_features 段`);
    }
    if (!hasStackPtrExport) {
        console.warn(`⚠ 警告: wasm 中缺少 __wbindgen_add_to_stack_pointer export`);
    }

    if (externrefCount === 0 && multivalueCount === 0 && targetFeaturesCount === 0 && hasStackPtrExport) {
        console.log('✓ wasm 已移除 externref + multivalue + target_features，已添加 stack_pointer 辅助函数（iOS 兼容）');
    }

    // 显示最终 exports
    const exports = checkOutput.match(/\(export ".*?"/g) || [];
    console.log(`  导出函数: ${exports.map(e => e.match(/"(.*?)"/)[1]).join(', ')}`);

} catch (e) {
    console.error('✗ wasm 后处理失败:', e.message);
    if (e.stderr) console.error('  stderr:', e.stderr.toString());
    console.warn('  需要安装 wasm-tools: cargo install wasm-tools');
    console.warn('  回退: 直接复制原始 wasm（iOS 可能不兼容）');
    fs.copyFileSync(wasmSrc, wasmDst);
}

// 复制 d.ts（可选）
const dtsPath = path.join(PKG_DIR, 'phone_scan_wasm.d.ts');
if (fs.existsSync(dtsPath)) {
    fs.copyFileSync(dtsPath, path.join(OUT_DIR, 'phone_scan_wasm.d.ts'));
}

console.log('✓ 小程序 WASM 包已生成到:', OUT_DIR);
console.log('  - phone_scan_wasm.js (CommonJS + WXWebAssembly, 无 typeof, retptr 调用模式)');
console.log('  - phone_scan_wasm_bg.wasm (无 externref, 无 multivalue, 无 target_features)');

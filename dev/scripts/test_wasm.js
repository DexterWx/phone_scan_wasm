/**
 * WASM 测试脚本 - Node.js 版本
 *
 * 用法: node test_wasm.js <配置文件路径> <输入图片路径> <输出目录>
 * 示例: node test_wasm.js ../../dev/test_data/cards/270716/test.json ../../dev/test_data/cards/270716/test.jpg ./output
 */

import init, { init_engine, inference_from_rgba } from '../../pkg/phone_scan_wasm.js';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { Jimp } from 'jimp';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function testWasm(configPath, inputImagePath, outputDir) {
    try {
        console.log('=== WASM 测试开始 ===\n');

        // 1. 初始化 WASM 模块
        console.log('1. 初始化 WASM 模块...');
        const wasmPath = path.join(__dirname, '../../pkg/phone_scan_wasm_bg.wasm');
        const wasmBuffer = fs.readFileSync(wasmPath);
        await init(wasmBuffer);
        console.log('✓ WASM 模块初始化成功\n');

        // 2. 读取配置文件
        console.log('2. 读取配置文件...');
        const configText = fs.readFileSync(configPath, 'utf-8');
        console.log(`✓ 配置文件读取成功: ${configPath}\n`);

        // 3. 初始化引擎
        console.log('3. 初始化识别引擎...');
        const initResult = JSON.parse(init_engine(configText));
        if (initResult.code !== 0) {
            console.error(`✗ 引擎初始化失败: ${initResult.message}`);
            process.exit(1);
        }
        console.log('✓ 引擎初始化成功\n');

        // 4. 读取输入图片并解码为 RGBA（模拟小程序 CameraFrame）
        console.log('4. 读取输入图片...');
        const jimpImage = await Jimp.read(inputImagePath);
        const width = jimpImage.width;
        const height = jimpImage.height;
        const rgbaData = new Uint8Array(jimpImage.bitmap.data);
        console.log(`✓ 图片读取成功: ${inputImagePath} (${width}x${height}, RGBA ${rgbaData.length} bytes)\n`);

        // 5. 执行识别（使用 RGBA 接口，和小程序调用方式一致）
        console.log('5. 执行识别...');
        const startTime = Date.now();
        const result = inference_from_rgba(rgbaData, width, height);
        const endTime = Date.now();
        console.log(`✓ 识别完成，耗时: ${endTime - startTime}ms\n`);

        // 6. 创建输出��录
        if (!fs.existsSync(outputDir)) {
            fs.mkdirSync(outputDir, { recursive: true });
        }

        // 7. 保存 JSON 结果
        console.log('6. 保存结果...');
        const jsonPath = path.join(outputDir, 'result.json');
        fs.writeFileSync(jsonPath, result.json);
        console.log(`✓ JSON 已保存: ${jsonPath}`);

        // 8. 保存图片（RGB 转 JPG 格式）
        const imagePath = path.join(outputDir, 'result.jpg');
        await saveRgbAsJpg(result.image_data, result.width, result.height, imagePath);
        console.log(`✓ 图片已保存: ${imagePath} (${result.width}x${result.height})`);

        // 9. 显示识别结果摘要
        console.log('\n=== 识别结果摘要 ===');
        const resultObj = JSON.parse(result.json);
        console.log(`code: ${resultObj.code}`);
        console.log(`message: ${resultObj.message}`);
        if (resultObj.code !== 0) {
            console.error('✗ 识别失败');
            process.exit(1);
        }
        console.log(`page_number: ${resultObj.page_number}`);
        console.log(`lpls: ${resultObj.lpls}`);
        console.log(`rec_results 数量: ${resultObj.rec_results.length}`);

        console.log('\n=== WASM 测试完成 ===');

    } catch (error) {
        console.error('\n✗ 测试失败:', error.message);
        console.error(error.stack);
        process.exit(1);
    }
}

// 将 RGB 数据保存为 JPG 格式
async function saveRgbAsJpg(rgbData, width, height, outputPath) {
    const image = new Jimp({ width, height });

    // 将 RGB 数据写入 Jimp 图像
    let idx = 0;
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const r = rgbData[idx++];
            const g = rgbData[idx++];
            const b = rgbData[idx++];
            const color = ((r << 24) | (g << 16) | (b << 8) | 0xFF) >>> 0;
            image.setPixelColor(color, x, y);
        }
    }

    await image.write(outputPath);
}

// 命令行参数解析
const args = process.argv.slice(2);
if (args.length < 3) {
    console.log('用法: node test_wasm.js <配置文件路径> <输入图片路径> <输出目录>');
    console.log('示例: node test_wasm.js ../../dev/test_data/cards/270716/test.json ../../dev/test_data/cards/270716/test.jpg ./output');
    process.exit(1);
}

const configPath = args[0];
const inputImagePath = args[1];
const outputDir = args[2];

// 检查文件是否存在
if (!fs.existsSync(configPath)) {
    console.error(`错误: 配置文件不存在: ${configPath}`);
    process.exit(1);
}

if (!fs.existsSync(inputImagePath)) {
    console.error(`错误: 输入图片不存在: ${inputImagePath}`);
    process.exit(1);
}

testWasm(configPath, inputImagePath, outputDir);

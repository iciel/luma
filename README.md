# Luma

本地图片压缩工具，支持 PNG → PNG/WebP、JPEG → WebP。图片不会上传到网络。

| 预设 | 适合什么情况 | 工作方式 |
|---|---|---|
| `quality`（默认） | 更在意压缩效果，可以等待 | 搜索多个候选并计算感知质量评分 |
| `fast` | 希望尽快完成 | PNG 生成一次调色板；WebP 按质量 80、method 3 编码一次 |

## 下载

在 [Windows release](https://github.com/iciel/luma/actions/workflows/windows.yml) 成功运行的 **Artifacts** 中下载 `luma-windows-x64`，解压后使用 `luma.exe`。压缩包包含许可证、源提交信息和 SHA-256 校验值。可将程序所在目录加入 PATH。

## 使用

```sh
luma input.png -o output.png
luma input.png -o output.png --preset fast
luma input.png -o output.webp --preset fast
luma photo.jpg -o output.webp --preset quality
luma input.png -o output.png --target 85
luma --help
```

输出名以 `.webp` 结尾时自动选择 WebP，也可以用 `--format png|webp` 明确指定。`--target` 仅用于 `quality`，是感知评分目标，不是编码器质量百分比；无法达到时可能返回最接近的结果，请检查 JSON 报告。

支持管道：

```sh
luma - -o - --preset fast --format webp
```

`-` 表示标准输入或输出。图片输出到 stdout 时，JSON 报告写到 stderr；输出到文件时，报告写到 stdout。失败返回非零退出码。

## 注意事项

- 压缩可能有损，不能保证小字或 AI 识别效果不变；`fast` 不评分，报告中的 `score` 为 `null`。
- PNG 输出不会比原始文件大，必要时返回原图；WebP 转换可能变大，需要调用方比较体积。
- 保留必要色彩信息。快速 PNG 清理无关元数据并保留方向；WebP 转换会应用 JPEG 方向，快速 WebP 也会应用 PNG 方向。
- `quality` 支持 APNG → APNG；快速 PNG 遇到动画或超过 800 万像素时返回原图。WebP 不接受动画，快速 WebP 也不接受超过 800 万像素的输入。
- 不提供 JPEG 编码。部分色彩配置无法转换成 WebP，会明确报错。
- stdin 上限 64 MiB。工具没有总耗时限制，调用方可设置进程超时。

## 开发验证

Windows CI 只构建 release 并打包，不运行测试或 Clippy。本地源码检查使用 `cargo fmt --check`；下载程序后执行：

```sh
python scripts/verify-fast.py --exe path/to/luma.exe --output dist/png-check
uv run --with pillow python scripts/verify-webp.py --exe path/to/luma.exe --output dist/webp-fast-check --preset fast
uv run --with pillow python scripts/verify-webp.py --exe path/to/luma.exe --output dist/webp-quality-check --preset quality
```

## 来源与许可

Luma 基于 [heydeer/pngdeer](https://github.com/heydeer/pngdeer) 的部分代码发展，保留原作者版权声明，采用 [MIT 许可证](LICENSE)。第三方组件遵循各自许可证；样图来源见 [SOURCES.md](bench/images/SOURCES.md)。

# Luma

本地图片压缩命令行工具。可以压缩 PNG，也可以将 PNG、JPEG 转成 WebP，图片不会上传到网络。

## 下载与安装

1. 登录 GitHub，打开 [Windows 构建列表](https://github.com/iciel/luma/actions/workflows/windows.yml)。
2. 进入一次成功的构建，在页面下方 **Artifacts** 中下载 `luma-windows-x64`。
3. 解压得到 `luma.exe`，将所在文件夹加入 PATH，即可在终端使用 `luma`。

目前提供 Windows x64 版本。下载包附带许可证、对应的 Git 提交和 SHA-256 校验值。

## 开始使用

```sh
# 压缩 PNG，默认使用 quality 模式
luma input.png -o output.png

# 更快地压缩 PNG
luma input.png -o output.png --preset fast

# 转成 WebP：适用于 PNG 和 JPEG
luma input.png -o output.webp --preset fast
luma photo.jpg -o output.webp
```

`-o` 指定输出文件，建议使用与原图不同的文件名。以 `.webp` 结尾时输出 WebP；也可用 `--format png` 或 `--format webp` 明确指定。

## 选择压缩模式

| 模式 | 适合什么情况 | 区别 |
|---|---|---|
| `quality`（默认） | 愿意花更多时间选择压缩结果 | 尝试多个结果，结合图片体积和视觉相似度评分选择输出 |
| `fast` | 更在意处理速度 | 使用固定策略，不反复搜索或计算相似度评分 |

通常先用默认模式；需要快速处理时，加上 `--preset fast`。两个模式都可能有损，`quality` 不保证每张图片都比 `fast` 更小。Luma 不主动缩小图片分辨率。

`quality` 默认自动选择评分目标。需要手动调整时可以使用：

```sh
luma input.png -o output.png --target 85
```

`85` 是视觉相似度的目标分数，不表示“保留 85% 画质”。分数越高通常越保守；达不到目标时可能选择最接近的结果，实际分数见处理报告。`fast` 不接受此参数。

## 格式和大图支持

| 输入 → 输出 | `quality` | `fast` |
|---|---|---|
| 静态 PNG → PNG | 支持，包括超过 800 万像素的图片 | 支持；超过 800 万像素时报错 |
| 静态 PNG / JPEG → WebP | 支持，包括超过 800 万像素的图片 | 支持；超过 800 万像素时报错 |
| 动画 PNG（APNG）→ PNG | 压缩并保留动画 | 不支持，会报错 |
| 动画 → WebP、输出 JPEG | 不支持 | 不支持 |

**800 万像素是 `fast` 的处理上限，不是整个工具的上限。** 像素数等于宽 × 高，例如 4000 × 3000 是 1200 万像素。`quality` 可以处理这类大图，但仍受内存和图片格式限制；WebP 的宽、高分别不能超过 16,383 像素。

**成功输出一定比输入文件小。** 两种模式、两种输出格式都检查最终文件的字节数。无法缩小时返回 `no_smaller_output` 错误，不创建或覆盖输出文件，也不向管道写出图片。可以尝试另一种模式或输出格式。

## 管道与处理报告

```sh
luma - -o - --preset fast --format webp
```

两个 `-` 分别表示从标准输入读取图片、向标准输出写入图片。此时处理报告写到标准错误，不会混入图片数据。使用文件输出时，报告显示在终端。

成功后会输出 JSON 报告，包含文件字节数；`quality` 还包含目标和实际评分，`fast` 的 `score: null` 表示没有评分。

标准输入最多接收 64 MiB。程序没有总耗时限制，自动化调用时可自行设置超时。运行失败时返回非零退出码；更多用法见 `luma --help`。

## 来源与许可

Luma 基于 [heydeer/pngdeer](https://github.com/heydeer/pngdeer) 的部分代码发展，保留原作者版权声明，采用 [MIT 许可证](LICENSE)。第三方组件遵循各自许可证；样图来源见 [SOURCES.md](bench/images/SOURCES.md)。

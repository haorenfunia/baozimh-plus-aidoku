# 包子漫画 Plus for Aidoku

这是一个 Aidoku 0.7+ 新格式中文源仓库，仅提供包子漫画 Plus 直连源，不包含 Proxy 源。

## 添加到 Aidoku

在 Aidoku 中添加下面的源列表地址：

```text
https://raw.githubusercontent.com/haorenfunia/baozimh-plus-aidoku/main/public/index.min.json
```

如果 raw.githubusercontent.com 访问不稳定，可以使用 jsDelivr：

```text
https://cdn.jsdelivr.net/gh/haorenfunia/baozimh-plus-aidoku@main/public/index.min.json
```

当前源：

- 包子漫画 Plus（直连）

源码位于 `sources/zh.baozimh1`，推送后由 GitHub Actions 自动打包。源内置 Plus 的 App 最新章节备用节点和 App 章节链接解析。

## v11 章节记录修复

章节 ID 不再包含会随更新而消失的 `|latest` 后缀，避免上一话从最新章变为普通章时丢失已读匹配。旧 ID 通过 Aidoku 的 `MigrationHandler` 迁移，漫画 ID 保持不变。

更新前请先创建 Aidoku 备份。更新时若提示源有重大变更，请选择“迁移”，然后刷新漫画目录。旧记录若仍存在，可映射到固定 ID；已经被删除的历史无法仅靠源恢复。全章节 App 节点请求和图片处理保持不变。

可独立验证 Rust/WASM 的旧 ID 迁移：

```sh
node sources/zh.baozimh1/tests/chapter_keys_wasm.mjs
```

需要 Rust 的 `wasm32-unknown-unknown` 目标。完整目录解析和源接口回归测试位于 `src/html/mod.rs` 和 `src/lib.rs`，需使用 Aidoku 测试运行环境执行。

## v12 官网首页

首页直接读取 `https://www.twmanga.com/`，按官网顺序展示全部栏目和全部条目，不使用分类接口拼出推荐。当前包含熱門漫畫、推薦國漫、推薦韓漫、推薦日漫、熱血漫畫、最新上架、最近更新；栏目名称和内容随官网刷新。

推荐栏使用 Aidoku 原生横向列表，最近更新显示官网的最新话提示。点击栏目可展开完整列表，顶部另有熱門漫畫、最新上架、最近更新、全部漫畫入口。官网首页当前没有轮播横幅，不添加其他网站的广告。界面由 Aidoku 原生组件绘制，内容与顺序对应官网，并非嵌入网页。

GitHub Actions 在发布前执行目录、迁移、首页解析回归测试，并额外逐栏比对实时官网的全部条目。实时检查单独报告，避免官网暂时无法访问时阻断已通过回归测试的构建。

## v13 简体显示与封面修复

首页栏目、书名、作者、标签、简介和章节名统一繁转简，转换在源内离线完成。内置 OpenCC 的完整繁转简字符与词组字典，保留“乾坤”“乾隆”等词组例外。漫画 ID、章节 ID、网址以及旧栏目 ID 保持不变，不需要再次迁移。

封面统一使用静态 CDN 原图，去掉可能卡住的动态缩图参数，首页、搜索、详情与已缓存地址的图片请求保持一致。封面请求与阅读图片请求分开，阅读图片的 App 请求头、全章节节点和图片处理不变。漫画图片内的文字不会转换。

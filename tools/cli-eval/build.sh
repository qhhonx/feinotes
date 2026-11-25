#!/bin/sh
# An evaluation library at $1 (a folder that does not exist yet): the
# guides library plus meeting notes, a plan, a proposal, a weekly report and
# a 数据表, made with `quiet` itself (QUIET_BIN, default target/release/quiet).
set -e
cd "$(dirname "$0")/../.."
Q=${QUIET_BIN:-$PWD/target/release/quiet}
out=$1
[ -n "$out" ] && [ ! -e "$out" ] || { echo "usage: $0 <new folder>" >&2; exit 2; }
cp -R shared-mock/library "$out"
export QUIET_MD_HOME="$out"
$Q mkdir 会议 >/dev/null
$Q mkdir 项目 >/dev/null
$Q new '2026-11-03 周会' --in 会议 --markdown --md '# 议题
- 讨论导出 PDF 的分页问题
- 搜索结果排序

# 待办
- [x] 张伟：整理导出问题清单
- [ ] 李娜：调研 PDF 分页方案
- [ ] 王强：给搜索加按标题优先排序' >/dev/null
$Q new '2026-11-10 周会' --in 会议 --markdown --md '# 议题
- 数据表的列宽
- 移动端阅读

# 待办
- [ ] 李娜：PDF 分页方案评审
- [x] 王强：搜索标题优先已上线
- [ ] 赵敏：移动端阅读体验走查' >/dev/null
$Q new '2026-11-17 产品评审' --in 会议 --md '# 结论
新版导出流程通过评审，下周灰度。

# 待办
- [ ] 张伟：准备灰度名单
- [ ] 赵敏：更新帮助文档里的导出章节
- [x] 李娜：确认分页方案' >/dev/null
$Q new 项目计划 --in 项目 --md '# 项目计划
整体目标：在 12 月底前发布 1.0。

## 需求调研
访谈 10 位用户，整理需求清单。

## 方案设计
输出交互稿和技术方案。

## 开发
按模块分三个迭代完成。

## 测试
回归测试和性能测试。

## 发布
灰度一周后全量发布。' >/dev/null
$Q new 方案 --in 项目 --md '# 导出功能改版方案

## 背景
现在的导出只支持 Markdown，用户经常要 PDF。

## 目标
- 支持 PDF 和 Word 导出
- 分页正确，图片不被截断

## 设计
导出前先排版，再按页切分；表格跨页时重复表头。

## 风险
- 大文档导出耗时长
- 字体授权问题' >/dev/null
$Q new 周报 --md '# 周报 2026-W47

## 本周进展
- 完成导出改版的技术方案
- 修复 3 个搜索相关的 bug

## 风险
- PDF 分页方案还没定
- 测试人力不足

## 下周计划
- 开始导出改版开发
- 灰度新版搜索' >/dev/null
$Q new 书单 --kind table --csv '书名,作者,评分,状态
三体,刘慈欣,5,读完
活着,余华,4,读完
围城,钱钟书,3,在读
人类简史,赫拉利,4,想读
小王子,圣埃克苏佩里,5,读完
' >/dev/null
echo "$($Q ls -r | wc -l) entries in $out"

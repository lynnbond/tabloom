//! All user-facing Chinese UI copy lives here.
pub const APP_NAME: &str = "Tabloom";
pub const WORKSPACE: &str = "临时工作区";
pub const REGION: &str = "区域";
pub const FILE: &str = "工作区";
pub const VIEW: &str = "外观";
pub const EDIT: &str = "编辑";
pub const FIND: &str = "查找";
pub const FIND_REPLACE: &str = "查找与替换";
pub const FIND_SCOPE: &str = "当前文档";
pub const FIND_QUERY: &str = "查找文本";
pub const REPLACEMENT: &str = "替换为";
pub const CASE_SENSITIVE: &str = "区分大小写";
pub const WHOLE_WORD: &str = "全词匹配";
pub const FIND_PREVIOUS: &str = "上一个";
pub const FIND_NEXT: &str = "下一个";
pub const CLOSE_FIND: &str = "关闭查找";
pub const REPLACE_CURRENT: &str = "替换当前";
pub const REPLACE_ALL: &str = "全部替换";
pub const MATCHES: &str = "处匹配";
pub const REPLACED: &str = "已替换";
pub const FIND_ERROR: &str = "查找失败：";
pub const COPY: &str = "复制";
pub const CUT: &str = "剪切";
pub const PASTE: &str = "粘贴纯文本";
pub const SELECT_ALL: &str = "全选";
pub const UNDO: &str = "撤销";
pub const REDO: &str = "重做";
pub const FONT_INCREASE: &str = "增大字号";
pub const FONT_DECREASE: &str = "减小字号";
pub const CLIPBOARD_ERROR: &str = "纯文本剪贴板操作失败，原文本已保留：";
pub const CLEAR_CLIPBOARD_ERROR: &str = "关闭剪贴板提示";
pub const NEW_WORKSPACE: &str = "新建工作区";
pub const CLOSE_WORKSPACE: &str = "关闭工作区";
pub const RECENTLY_CLOSED: &str = "最近关闭";
pub const NONE_CLOSED: &str = "暂无关闭的工作区";
pub const NEW_REGION: &str = "新建区域";
pub const MOVE_UP: &str = "上移";
pub const MOVE_DOWN: &str = "下移";
pub const DELETE: &str = "删除";
pub const FOCUS: &str = "聚焦";
pub const SYNTAX: &str = "语法";
pub const EDITOR: &str = "纯文本编辑区";
pub const RESIZE: &str = "拖动调整区域高度";
pub const DARK: &str = "深色主题";
pub const LIGHT: &str = "浅色主题";
pub const FONT_SIZE: &str = "字号";
pub const MEMORY_WARNING: &str = "独立测试模式：仅使用内存，不访问正式工作区文件。";
pub const MEMORY_STATUS: &str = "仅在内存中 · 未保存";
pub const EMPTY_HINT: &str = "开始输入…";
pub const DELETE_TITLE: &str = "删除编辑区域？";
pub const DELETE_MESSAGE: &str = "该区域的内容将被移除，此操作不能撤销。";
pub const CANCEL: &str = "取消";
pub const CONFIRM_DELETE: &str = "确认删除区域";
pub const EXIT_TITLE: &str = "尚有未保存的更改";
pub const EXIT_MESSAGE: &str = "更改尚未成功写入本地工作区文件。可重试或复制恢复副本后粘贴到外部文件；直接退出会丢弃未写入的部分。";
pub const CONFIRM_EXIT: &str = "放弃未保存更改并退出";
pub const RETRY_AND_EXIT: &str = "重试保存并退出";
pub const SAVED: &str = "已自动保存到本地";
pub const SAVE_PENDING: &str = "等待自动保存…";
pub const SAVE_FAILED: &str = "保存失败 · 更改仍在内存";
pub const SAVE_ERROR: &str = "自动保存失败（编辑仍可继续）：";
pub const BACKUP_ERROR: &str = "内容已保存，但备份失败：";
pub const MAINTENANCE_ERROR: &str = "内容已保存，但旧版本文件清理失败：";
pub const RETRY_MAINTENANCE: &str = "重试清理文件";
pub const MAINTENANCE_EXIT_TITLE: &str = "内容已保存，文件清理尚未完成";
pub const MAINTENANCE_EXIT_MESSAGE: &str = "旧版本文件清理未完成。可以重试，或直接退出；已提交的工作区会在下次启动时恢复。请保留数据目录。";
pub const EXIT_COMMITTED: &str = "直接退出（保留已提交数据）";
pub const RETRY_SAVE: &str = "重试保存";
pub const RETRY_BACKUP: &str = "重试备份";
pub const COPY_RESCUE: &str = "复制全部恢复数据（JSON）";
pub const RESCUE_ERROR: &str = "生成恢复数据失败：";
pub const RECOVERY_NOTICE: &str = "已从有效备份恢复。恢复的是备份时点内容，原始数据文件保留于：";
pub const DISMISS: &str = "知道了";
pub const PERMANENT_DELETE: &str = "永久删除";
pub const PERMANENT_DELETE_TITLE: &str = "永久删除已关闭的工作区？";
pub const PERMANENT_DELETE_MESSAGE: &str =
    "将从当前本地工作区删除该工作区及全部区域，无法从最近关闭找回。滚动备份中可能仍有旧副本。";
pub const CONFIRM_PERMANENT_DELETE: &str = "确认永久删除工作区";
pub const HIGHLIGHT_ERROR: &str = "高亮失败，当前以纯文本显示：";
pub const LOG_NOTE: &str = "日志请使用纯文本；本版未提供通用日志高亮规则。";
pub const LINES: &str = "行";
pub const CHARACTERS: &str = "字符";

pub const OPEN_FILE: &str = "打开文本文件";
pub const SAVE_FILE: &str = "保存关联文件";
pub const EXPORT_FILE: &str = "导出当前区域（UTF-8）";
pub const FILE_SAVED: &str = "已写入文件：";
pub const FILE_EXPORTED: &str = "已导出纯文本副本：";
pub const FILE_OPENED: &str = "已打开文件：";
pub const TEMP_SAVED: &str = "当前临时区域已自动保存到本地工作区；需要文件时请使用导出当前区域。";
pub const BEFORE_FILE_WRITE_ERROR: &str =
    "内部恢复副本保存失败，本次未打开文件对话框或写入真实文件；可继续编辑并重试";
pub const AFTER_FILE_WRITE_ERROR: &str = "真实文件已写入，但内部文件状态保存失败，请重试自动保存";
pub const OPEN_RECOVERY_ERROR: &str = "文件已在编辑器打开，但内部恢复副本保存失败";
pub const CURSOR_LINE: &str = "行";
pub const CURSOR_COLUMN: &str = "列";

pub const EXPORT_RECOVERY_ERROR: &str = "内部恢复副本仍未保存，请修复后重试：";

pub const LARGE_TEXT_NOTE: &str = "较大文本或超长行已暂停语法着色；文本和语言设置保持原样。";

pub const DOCUMENT_EDITOR: &str = "文档编辑区";
pub const EXTRACT_SELECTION: &str = "将选中整行设为区域";
pub const WHOLE_LINE_HINT: &str = "请完整选中一行或多行；可以不选中行尾换行符。";
pub const SPAN_TOOLS: &str = "区域操作";

pub const THEME_GRAPHITE: &str = "A · 石墨极简";
pub const THEME_PAPER: &str = "B · 纸白原生";
pub const THEME_OCEAN: &str = "C · 深海工作台";
pub const THEME_SAND: &str = "D · 暖砂留白";

pub const RECOVERY_EDITING: &str =
    "原工作区暂不可用，已打开独立恢复工作区，可以正常编辑和自动保存。";
pub const MEMORY_RECOVERY: &str =
    "本地存储暂不可用，仍可编辑；本次内容仅在内存，请导出或复制恢复副本。";
pub const STORAGE_DETAILS: &str = "保存位置与详细原因";
pub const RECOVERY_KEEP: &str =
    "原数据保持不变。恢复工作区不会自动覆盖或合并到原工作区；请保留显示的保存目录。";
pub const RECOVERY_SAVED: &str = "已保存到恢复工作区";

pub const CHOOSE_DIRECTORY: &str = "选择工作区目录…";
pub const REVEAL_DIRECTORY: &str = "打开保存目录";
pub const STORAGE_LOCATION: &str = "保存位置";
pub const USE_RECOVERY_DIRECTORY: &str = "以后使用当前恢复目录";
pub const DIRECTORY_HELP: &str =
    "请选择专用文件夹。空目录会保存当前所有标签；已有工作区会先保存当前内容再切换。";
pub const DIRECTORY_PICKER: &str = "选择专用工作区目录";
pub const DIRECTORY_SAVED: &str = "当前全部标签已保存到所选目录：";
pub const DIRECTORY_OPENED: &str = "已打开所选工作区，原工作区保留在原目录：";
pub const DIRECTORY_REMEMBER_FAILED: &str = "本次目录已切换，但未能记住下次启动位置：";
pub const DIRECTORY_UNSAVED: &str =
    "当前内容无法保存，请先选择空目录保存当前全部标签，再打开已有工作区。";
pub const NO_DIRECTORY: &str = "当前内容仅在内存；请选择工作区目录保存。";

pub const FORMAT_SELECTION: &str = "设置选中行格式";
pub const CURRENT_FORMAT: &str = "格式";
pub const MIXED_FORMAT: &str = "混合格式";

pub const ABOUT_APP: &str = "关于 Tabloom";
pub const HIDE_APP: &str = "隐藏 Tabloom";
pub const HIDE_OTHERS: &str = "隐藏其他应用";
pub const SHOW_ALL: &str = "显示全部";
pub const QUIT_APP: &str = "退出 Tabloom";
pub const WINDOW: &str = "窗口";
pub const MINIMIZE: &str = "最小化";
pub const FULLSCREEN: &str = "进入/退出全屏";
pub const NATIVE_MENU_ERROR: &str = "系统菜单更新失败，已恢复窗口菜单：";

pub const MARKDOWN_PREVIEW: &str = "Markdown 预览（双击编辑源码）";
pub const MARKDOWN_SOURCE: &str = "Markdown 源码";
pub const REGION_SOURCE: &str = "文本源码";
pub const EDIT_MARKDOWN: &str = "编辑 Markdown 源码";
pub const COPY_MARKDOWN: &str = "复制 Markdown 源码";
pub const MARKDOWN_LARGE: &str = "Markdown 超过 256 KiB，显示源码以保持编辑流畅。";
pub const DOCUMENT_SOURCE: &str = "编辑整篇源码";
pub const DOCUMENT_PREVIEW: &str = "恢复 Markdown 预览";

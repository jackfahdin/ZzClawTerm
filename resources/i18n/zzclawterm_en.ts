<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="en" sourcelanguage="zh_CN">
<context>
    <name>ZzAboutDialog</name>
    <message>
        <location filename="../../src/dialog/ZzAboutDialog.cpp" line="29"/>
        <source>版本 %1 · %2 构建 · %3</source>
        <translation>Version %1 · %2 build · %3</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzAboutDialog.cpp" line="39"/>
        <source>基于 Qt %1</source>
        <translation>Based on Qt %1</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzAboutDialog.cpp" line="59"/>
        <source>关于 ZzClawTerm</source>
        <translation>About ZzClawTerm</translation>
    </message>
</context>
<context>
    <name>ZzAppShell</name>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="54"/>
        <source>已连接</source>
        <translation>Connected</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="56"/>
        <source>连接中…</source>
        <translation>Connecting…</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="58"/>
        <location filename="../../src/ZzAppShell.cpp" line="60"/>
        <source>未连接</source>
        <translation>Disconnected</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="129"/>
        <source>会话</source>
        <translation>Sessions</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="148"/>
        <source>文件</source>
        <translation>File</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="169"/>
        <source>导航</source>
        <translation>Navigation</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="189"/>
        <location filename="../../src/ZzAppShell.cpp" line="403"/>
        <source>隧道: %1</source>
        <translation>Tunnel: %1</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="377"/>
        <source>X server 异常退出：%1</source>
        <translation>X server exited unexpectedly: %1</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="435"/>
        <source>会话面板不可用</source>
        <translation>Session panel unavailable</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="442"/>
        <source>会话面板尚未就绪</source>
        <translation>Session panel is not ready yet</translation>
    </message>
    <message>
        <location filename="../../src/ZzAppShell.cpp" line="456"/>
        <source>无法打开设置页</source>
        <translation>Unable to open the settings page</translation>
    </message>
</context>
<context>
    <name>ZzForwardRule</name>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="18"/>
        <location filename="../../src/session/ZzForwardRule.cpp" line="24"/>
        <source>本地</source>
        <translation>Local</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="20"/>
        <source>远程</source>
        <translation>Remote</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="22"/>
        <source>动态</source>
        <translation>Dynamic</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="73"/>
        <source>监听地址不能为空</source>
        <translation>Listen address must not be empty</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="75"/>
        <source>监听端口必须在 1-65535 之间</source>
        <translation>Listen port must be between 1 and 65535</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="78"/>
        <source>本地/远程转发必须填写目标地址</source>
        <translation>Local/Remote forwarding requires a target address</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="80"/>
        <source>目标端口必须在 1-65535 之间</source>
        <translation>Target port must be between 1 and 65535</translation>
    </message>
    <message>
        <location filename="../../src/session/ZzForwardRule.cpp" line="92"/>
        <source>存在重复的转发规则：%1 %2:%3</source>
        <translation>Duplicate forwarding rule: %1 %2:%3</translation>
    </message>
</context>
<context>
    <name>ZzHostKeyDialog</name>
    <message>
        <location filename="../../src/dialog/ZzHostKeyDialog.cpp" line="12"/>
        <source>警告：主机密钥已变更</source>
        <translation>Warning: Host Key Changed</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzHostKeyDialog.cpp" line="13"/>
        <source>主机 %1 的密钥指纹与本地记录不一致！

旧指纹（本地记录）：%2
新指纹（服务器上报）：%3

这可能意味着中间人攻击或服务器重装。确认无误后才可继续。</source>
        <translation>The key fingerprint of host %1 does not match the local record!

Old fingerprint (local record): %2
New fingerprint (reported by server): %3

This may indicate a man-in-the-middle attack or a server reinstall. Continue only after verifying the fingerprint.</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzHostKeyDialog.cpp" line="25"/>
        <source>确认主机密钥</source>
        <translation>Confirm Host Key</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzHostKeyDialog.cpp" line="26"/>
        <source>首次连接主机 %1。

密钥指纹：%2

是否信任并保存该指纹？</source>
        <translation>Connecting to host %1 for the first time.

Key fingerprint: %2

Trust and save this fingerprint?</translation>
    </message>
</context>
<context>
    <name>ZzLocalPtyTransport</name>
    <message>
        <location filename="../../src/transport/ZzLocalPtyTransport.cpp" line="71"/>
        <source>本地 shell 已退出（退出码 %1）</source>
        <translation>Local shell exited (exit code %1)</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzLocalPtyTransport.cpp" line="88"/>
        <source>启动本地 shell 失败：%1</source>
        <translation>Failed to start local shell: %1</translation>
    </message>
</context>
<context>
    <name>ZzLocalShellConfigPage</name>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="91"/>
        <source>名称不能为空</source>
        <translation>Name cannot be empty</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="120"/>
        <source>常规</source>
        <translation>General</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="132"/>
        <source>名称：</source>
        <translation>Name:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="133"/>
        <source>分组路径：</source>
        <translation>Group Path:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="134"/>
        <source>Shell 程序：</source>
        <translation>Shell Program:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="136"/>
        <source>如：生产环境/Web 服务器</source>
        <translation>e.g. Production/Web Server</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzLocalShellConfigPage.cpp" line="137"/>
        <source>留空使用系统默认 shell</source>
        <translation>Leave empty to use the system default shell</translation>
    </message>
</context>
<context>
    <name>ZzLogArchiveWorker</name>
    <message>
        <location filename="../../src/log/ZzLogArchiveWorker.cpp" line="74"/>
        <source>冷层写入失败（已重试 3 次）：%1</source>
        <translation>Cold-tier write failed (retried 3 times): %1</translation>
    </message>
</context>
<context>
    <name>ZzLogEngine</name>
    <message>
        <location filename="../../src/log/ZzLogEngine.cpp" line="60"/>
        <source>冷层库打开失败，降级为温层模式：%1</source>
        <translation>Failed to open cold-tier store; falling back to warm-tier mode: %1</translation>
    </message>
    <message>
        <location filename="../../src/log/ZzLogEngine.cpp" line="79"/>
        <source>温层文件打开失败，降级为纯内存模式：%1</source>
        <translation>Failed to open warm-tier file; falling back to in-memory mode: %1</translation>
    </message>
    <message>
        <location filename="../../src/log/ZzLogEngine.cpp" line="149"/>
        <source>残留温层续传失败，降级为温层模式</source>
        <translation>Failed to resume leftover warm-tier file; falling back to warm-tier mode</translation>
    </message>
</context>
<context>
    <name>ZzMasterPasswordDialog</name>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="38"/>
        <source>两次输入不一致</source>
        <translation>The two entries do not match</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="45"/>
        <source>密码错误或为空</source>
        <translation>Password is incorrect or empty</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="56"/>
        <source>设置主密码</source>
        <translation>Set Master Password</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="57"/>
        <source>解锁凭据库</source>
        <translation>Unlock Credential Store</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="59"/>
        <source>首次使用凭据存储，请设置主密码（AES-256-GCM 加密，规格 §6.2）：</source>
        <translation>This is the first time the credential store is used. Please set a master password (AES-256-GCM encryption, spec §6.2):</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="60"/>
        <source>请输入主密码解锁凭据库：</source>
        <translation>Enter the master password to unlock the credential store:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="68"/>
        <source>主密码：</source>
        <translation>Master Password:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzMasterPasswordDialog.cpp" line="70"/>
        <source>确认密码：</source>
        <translation>Confirm Password:</translation>
    </message>
</context>
<context>
    <name>ZzMenuBarService</name>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="174"/>
        <source>会话(&amp;S)</source>
        <translation>Session(&amp;S)</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="176"/>
        <source>新建会话(&amp;N)</source>
        <translation>New Session(&amp;N)</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="178"/>
        <source>视图(&amp;V)</source>
        <translation>View(&amp;V)</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="179"/>
        <source>主题</source>
        <translation>Theme</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="181"/>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="193"/>
        <source>跟随系统</source>
        <translation>Follow System</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="182"/>
        <source>浅色</source>
        <translation>Light</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="183"/>
        <source>深色</source>
        <translation>Dark</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="184"/>
        <source>高对比</source>
        <translation>High Contrast</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="189"/>
        <source>终端主题</source>
        <translation>Terminal Theme</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="190"/>
        <source>更多方案…</source>
        <translation>More Color Schemes…</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="191"/>
        <source>语言</source>
        <translation>Language</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="194"/>
        <source>简体中文</source>
        <translation>简体中文</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="195"/>
        <source>English</source>
        <translation>English</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="201"/>
        <source>帮助(&amp;H)</source>
        <translation>Help(&amp;H)</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="203"/>
        <source>关于 ZzClawTerm(&amp;A)</source>
        <translation>About ZzClawTerm(&amp;A)</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="205"/>
        <source>打开日志目录</source>
        <translation>Open Log Directory</translation>
    </message>
    <message>
        <location filename="../../src/menu/ZzMenuBarService.cpp" line="207"/>
        <source>GitHub 仓库</source>
        <translation>GitHub Repository</translation>
    </message>
</context>
<context>
    <name>ZzSessionConfigWindow</name>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="96"/>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="103"/>
        <source>输入无效</source>
        <translation>Invalid Input</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="126"/>
        <source>新建会话</source>
        <translation>New Session</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="126"/>
        <source>编辑会话</source>
        <translation>Edit Session</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="127"/>
        <source>SSH</source>
        <translation>SSH</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="128"/>
        <source>本地 Shell</source>
        <translation>Local Shell</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="154"/>
        <source>密码未保存</source>
        <translation>Password Not Saved</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="155"/>
        <source>凭据库未解锁，密码未保存。
请解锁凭据库后重试，或改用其他认证方式。</source>
        <translation>The credential store is locked, so the password was not saved.
Unlock the credential store and try again, or use another authentication method.</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="192"/>
        <source>私钥口令未保存</source>
        <translation>Private Key Passphrase Not Saved</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSessionConfigWindow.cpp" line="193"/>
        <source>凭据库未解锁，私钥口令未保存。
请解锁凭据库后重试，或留空口令。</source>
        <translation>The credential store is locked, so the private key passphrase was not saved.
Unlock the credential store and try again, or leave the passphrase empty.</translation>
    </message>
</context>
<context>
    <name>ZzSessionEditDialog</name>
    <message>
        <source>新建会话</source>
        <translation type="vanished">New Session</translation>
    </message>
    <message>
        <source>编辑会话</source>
        <translation type="vanished">Edit Session</translation>
    </message>
    <message>
        <source>名称：</source>
        <translation type="vanished">Name:</translation>
    </message>
    <message>
        <source>分组路径：</source>
        <translation type="vanished">Group Path:</translation>
    </message>
    <message>
        <source>协议：</source>
        <translation type="vanished">Protocol:</translation>
    </message>
    <message>
        <source>主机：</source>
        <translation type="vanished">Host:</translation>
    </message>
    <message>
        <source>端口：</source>
        <translation type="vanished">Port:</translation>
    </message>
    <message>
        <source>Shell 程序：</source>
        <translation type="vanished">Shell Program:</translation>
    </message>
    <message>
        <source>用户名：</source>
        <translation type="vanished">Username:</translation>
    </message>
    <message>
        <source>认证方式：</source>
        <translation type="vanished">Authentication:</translation>
    </message>
    <message>
        <source>私钥路径：</source>
        <translation type="vanished">Private Key Path:</translation>
    </message>
    <message>
        <source>私钥口令：</source>
        <translation type="vanished">Private Key Passphrase:</translation>
    </message>
    <message>
        <source>密码：</source>
        <translation type="vanished">Password:</translation>
    </message>
    <message>
        <source>端口转发：</source>
        <translation type="vanished">Port Forwarding:</translation>
    </message>
    <message>
        <source>图形转发：</source>
        <translation type="vanished">X11 Forwarding:</translation>
    </message>
    <message>
        <source>如：生产环境/Web 服务器</source>
        <translation type="vanished">e.g. Production/Web Server</translation>
    </message>
    <message>
        <source>留空使用系统默认 shell</source>
        <translation type="vanished">Leave empty to use the system default shell</translation>
    </message>
    <message>
        <source>私钥路径（公钥认证）</source>
        <translation type="vanished">Private key path (public key authentication)</translation>
    </message>
    <message>
        <source>私钥口令（无口令留空）</source>
        <translation type="vanished">Private key passphrase (leave empty if none)</translation>
    </message>
    <message>
        <source>留空保留已保存的口令</source>
        <translation type="vanished">Leave empty to keep the saved passphrase</translation>
    </message>
    <message>
        <source>登录密码</source>
        <translation type="vanished">Login password</translation>
    </message>
    <message>
        <source>留空保留已保存的密码</source>
        <translation type="vanished">Leave empty to keep the saved password</translation>
    </message>
    <message>
        <source>SSH</source>
        <translation type="vanished">SSH</translation>
    </message>
    <message>
        <source>本地 Shell</source>
        <translation type="vanished">Local Shell</translation>
    </message>
    <message>
        <source>SSH Agent</source>
        <translation type="vanished">SSH Agent</translation>
    </message>
    <message>
        <source>公钥文件</source>
        <translation type="vanished">Public Key File</translation>
    </message>
    <message>
        <source>密码</source>
        <translation type="vanished">Password</translation>
    </message>
    <message>
        <source>类型</source>
        <translation type="vanished">Type</translation>
    </message>
    <message>
        <source>监听地址</source>
        <translation type="vanished">Listen Address</translation>
    </message>
    <message>
        <source>监听端口</source>
        <translation type="vanished">Listen Port</translation>
    </message>
    <message>
        <source>目标地址</source>
        <translation type="vanished">Target Address</translation>
    </message>
    <message>
        <source>目标端口</source>
        <translation type="vanished">Target Port</translation>
    </message>
    <message>
        <source>本地 -L</source>
        <translation type="vanished">Local -L</translation>
    </message>
    <message>
        <source>远程 -R</source>
        <translation type="vanished">Remote -R</translation>
    </message>
    <message>
        <source>动态 -D</source>
        <translation type="vanished">Dynamic -D</translation>
    </message>
    <message>
        <source>添加</source>
        <translation type="vanished">Add</translation>
    </message>
    <message>
        <source>删除</source>
        <translation type="vanished">Delete</translation>
    </message>
    <message>
        <source>X11 转发</source>
        <translation type="vanished">X11 Forwarding</translation>
    </message>
    <message>
        <source>Windows 端首次使用将下载内建 X server；Linux/macOS 需本机 X server / XQuartz</source>
        <translation type="vanished">On Windows, the built-in X server is downloaded on first use; on Linux/macOS a local X server / XQuartz is required</translation>
    </message>
    <message>
        <source>嵌入标签页显示（实验；否则独立窗口）</source>
        <translation type="vanished">Embed in tab (experimental; otherwise shown in a separate window)</translation>
    </message>
    <message>
        <source>仅 Windows 生效：X11 桌面嵌入会话标签页内；取消勾选则 X 程序以独立窗口显示</source>
        <translation type="vanished">Windows only: the X11 desktop is embedded in the session tab; if unchecked, X programs are shown in separate windows</translation>
    </message>
    <message>
        <source>主机不能为空</source>
        <translation type="vanished">Host cannot be empty</translation>
    </message>
    <message>
        <source>名称不能为空</source>
        <translation type="vanished">Name cannot be empty</translation>
    </message>
    <message>
        <source>转发规则无效</source>
        <translation type="vanished">Invalid forwarding rule</translation>
    </message>
    <message>
        <source>密码未保存</source>
        <translation type="vanished">Password Not Saved</translation>
    </message>
    <message>
        <source>凭据库未解锁，密码未保存。
请解锁凭据库后重试，或改用其他认证方式。</source>
        <translation type="vanished">The credential store is locked, so the password was not saved.
Unlock the credential store and try again, or use another authentication method.</translation>
    </message>
    <message>
        <source>私钥口令未保存</source>
        <translation type="vanished">Private Key Passphrase Not Saved</translation>
    </message>
    <message>
        <source>凭据库未解锁，私钥口令未保存。
请解锁凭据库后重试，或留空口令。</source>
        <translation type="vanished">The credential store is locked, so the private key passphrase was not saved.
Unlock the credential store and try again, or leave the passphrase empty.</translation>
    </message>
</context>
<context>
    <name>ZzSessionPanel</name>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="56"/>
        <source>会话</source>
        <translation>Sessions</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="137"/>
        <source>本地 Shell</source>
        <translation>Local Shell</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="157"/>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="181"/>
        <source>新建会话</source>
        <translation>New Session</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="168"/>
        <source>在此分组新建会话</source>
        <translation>New Session in This Group</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="182"/>
        <source>编辑</source>
        <translation>Edit</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="183"/>
        <source>删除</source>
        <translation>Delete</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="184"/>
        <source>复制</source>
        <translation>Duplicate</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSessionPanel.cpp" line="230"/>
        <source>（副本）</source>
        <translation> (Copy)</translation>
    </message>
</context>
<context>
    <name>ZzSettingsPage</name>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="129"/>
        <source>终端类型：</source>
        <translation>Terminal Type:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="130"/>
        <source>默认编码：</source>
        <translation>Default Encoding:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="131"/>
        <source>字号：</source>
        <translation>Font Size:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="132"/>
        <source>配色方案：</source>
        <translation>Color Scheme:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="133"/>
        <source>内存历史行数：</source>
        <translation>In-Memory History Lines:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="134"/>
        <source>凭据后端：</source>
        <translation>Credential Backend:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="135"/>
        <source>X11：</source>
        <translation>X11:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="136"/>
        <source>SFTP 块大小：</source>
        <translation>SFTP Block Size:</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="138"/>
        <source>自动（优先系统密钥环）</source>
        <translation>Auto (prefer system keyring)</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="139"/>
        <source>AES 加密文件</source>
        <translation>AES-Encrypted File</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="140"/>
        <source>系统密钥环</source>
        <translation>System Keyring</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="142"/>
        <source>凭据库在应用启动时构造，本项改动重启后生效。
切换到系统密钥环不会自动迁移旧 AES 文件中的凭据（旧文件保留不删）。</source>
        <translation>The credential store is created at application startup; changes to this option take effect after a restart.
Switching to the system keyring does not automatically migrate credentials from the old AES file (the old file is kept).</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="145"/>
        <source>启用 X server（启动时自动运行）</source>
        <translation>Enable X server (run automatically at startup)</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="147"/>
        <source>关闭后停止内建 X server，新会话不再发起 X11 转发；重新开启即恢复</source>
        <translation>When disabled, the built-in X server is stopped and new sessions no longer request X11 forwarding; re-enable to restore it</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="149"/>
        <source>自动（BDP 自适应）</source>
        <translation>Auto (BDP adaptive)</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="151"/>
        <source>手动值对高延迟链路可能更优；自动模式按链路 RTT 自适应（推荐）。
进行中的传输不受影响，下一传输生效。</source>
        <translation>A manual value may be better for high-latency links; auto mode adapts to the link RTT (recommended).
Ongoing transfers are not affected; the change applies to the next transfer.</translation>
    </message>
    <message>
        <location filename="../../src/settings/ZzSettingsPage.cpp" line="155"/>
        <source>改动立即生效：新标签使用新值，已打开标签实时应用字号/配色/编码。</source>
        <translation>Changes take effect immediately: new tabs use the new values, and open tabs apply font size, color scheme, and encoding in real time.</translation>
    </message>
</context>
<context>
    <name>ZzSftpPanel</name>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="169"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="308"/>
        <source>无活动会话</source>
        <translation>No active session</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="199"/>
        <source>上级</source>
        <translation>Up</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="200"/>
        <source>返回上级目录</source>
        <translation>Go to parent directory</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="201"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="936"/>
        <source>刷新</source>
        <translation>Refresh</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="202"/>
        <source>刷新当前目录</source>
        <translation>Refresh current directory</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="203"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="565"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="593"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="930"/>
        <source>上传</source>
        <translation>Upload</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="204"/>
        <source>上传本地文件（可多选）</source>
        <translation>Upload local files (multi-select supported)</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="205"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="829"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="932"/>
        <source>上传文件夹</source>
        <translation>Upload Folder</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="206"/>
        <source>递归上传本地文件夹</source>
        <translation>Recursively upload a local folder</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="207"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="578"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="605"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="918"/>
        <source>下载</source>
        <translation>Download</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="208"/>
        <source>下载选中文件</source>
        <translation>Download selected files</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="209"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="870"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="934"/>
        <source>新建目录</source>
        <translation>New Directory</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="210"/>
        <source>在当前目录新建子目录</source>
        <translation>Create a subdirectory in the current directory</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="211"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="885"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="926"/>
        <source>删除</source>
        <translation>Delete</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="212"/>
        <source>删除选中文件/空目录</source>
        <translation>Delete selected files/empty directories</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="213"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="900"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="924"/>
        <source>重命名</source>
        <translation>Rename</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="214"/>
        <source>重命名选中条目</source>
        <translation>Rename the selected item</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="216"/>
        <source>远端路径，回车跳转</source>
        <translation>Remote path; press Enter to jump</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="217"/>
        <source>名称</source>
        <translation>Name</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="217"/>
        <source>大小</source>
        <translation>Size</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="218"/>
        <source>权限</source>
        <translation>Permissions</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="218"/>
        <source>修改时间</source>
        <translation>Modified</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="219"/>
        <source>文件</source>
        <translation>File</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="219"/>
        <source>方向</source>
        <translation>Direction</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="219"/>
        <source>进度</source>
        <translation>Progress</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="220"/>
        <source>状态</source>
        <translation>Status</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="220"/>
        <source>操作</source>
        <translation>Actions</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="295"/>
        <source>正在打开 SFTP 会话…</source>
        <translation>Opening SFTP session…</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="310"/>
        <source>当前会话为本地 Shell，SFTP 不可用</source>
        <translation>The current session is a local shell; SFTP is unavailable</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="312"/>
        <source>等待 SSH 连接…</source>
        <translation>Waiting for SSH connection…</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="362"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="676"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="701"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="709"/>
        <source>进行中</source>
        <translation>In Progress</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="363"/>
        <source>已中断</source>
        <translation>Interrupted</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="378"/>
        <source>SFTP 会话错误：%1</source>
        <translation>SFTP session error: %1</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="389"/>
        <source>SFTP 会话已关闭</source>
        <translation>SFTP session closed</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="405"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="410"/>
        <source>SFTP 会话未打开</source>
        <translation>SFTP session not open</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="419"/>
        <source>加载 %1 …</source>
        <translation>Loading %1 …</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="484"/>
        <source>%1 项</source>
        <translation>%1 items</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="664"/>
        <source>操作失败：%1</source>
        <translation>Operation failed: %1</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="685"/>
        <source>取消</source>
        <translation>Cancel</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="731"/>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="784"/>
        <source>已取消</source>
        <translation>Cancelled</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="764"/>
        <source>完成</source>
        <translation>Completed</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="786"/>
        <source>失败：%1</source>
        <translation>Failed: %1</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="823"/>
        <source>上传文件</source>
        <translation>Upload Files</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="840"/>
        <source>请选择要下载的文件</source>
        <translation>Select the files to download</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="844"/>
        <source>下载到</source>
        <translation>Download To</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="856"/>
        <source>请选择要下载的目录</source>
        <translation>Select the directory to download</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="860"/>
        <source>下载文件夹到</source>
        <translation>Download Folder To</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="870"/>
        <source>目录名：</source>
        <translation>Directory Name:</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="886"/>
        <source>确定删除 %1 吗？</source>
        <translation>Delete %1?</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="900"/>
        <source>新名称：</source>
        <translation>New Name:</translation>
    </message>
    <message>
        <location filename="../../src/panel/ZzSftpPanel.cpp" line="921"/>
        <source>下载文件夹</source>
        <translation>Download Folder</translation>
    </message>
</context>
<context>
    <name>ZzSshConfigPage</name>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="135"/>
        <source>选择私钥文件</source>
        <translation>Select Private Key File</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="294"/>
        <source>名称不能为空</source>
        <translation>Name cannot be empty</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="299"/>
        <source>主机不能为空</source>
        <translation>Host cannot be empty</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="361"/>
        <source>常规</source>
        <translation>General</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="361"/>
        <source>连接</source>
        <translation>Connection</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="361"/>
        <source>认证</source>
        <translation>Authentication</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="362"/>
        <source>端口转发</source>
        <translation>Port Forwarding</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="362"/>
        <source>X11</source>
        <translation>X11</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="362"/>
        <source>终端</source>
        <translation>Terminal</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="378"/>
        <source>名称：</source>
        <translation>Name:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="379"/>
        <source>分组路径：</source>
        <translation>Group Path:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="383"/>
        <source>主机：</source>
        <translation>Host:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="384"/>
        <source>端口：</source>
        <translation>Port:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="385"/>
        <source>终端类型：</source>
        <translation>Terminal Type:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="386"/>
        <source>编码：</source>
        <translation>Encoding:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="387"/>
        <source>保活间隔：</source>
        <translation>Keepalive Interval:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="391"/>
        <source>用户名：</source>
        <translation>Username:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="392"/>
        <source>认证方式：</source>
        <translation>Authentication:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="394"/>
        <source>私钥路径：</source>
        <translation>Private Key Path:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="395"/>
        <source>私钥口令：</source>
        <translation>Private Key Passphrase:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="396"/>
        <source>密码：</source>
        <translation>Password:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="400"/>
        <source>图形转发：</source>
        <translation>X11 Forwarding:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="401"/>
        <source>显示方式：</source>
        <translation>Display Mode:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="405"/>
        <source>配色方案：</source>
        <translation>Color Scheme:</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="407"/>
        <source>如：生产环境/Web 服务器</source>
        <translation>e.g. Production/Web Server</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="408"/>
        <source>私钥路径（公钥认证）</source>
        <translation>Private key path (public key authentication)</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="412"/>
        <source>留空保留已保存的口令</source>
        <translation>Leave empty to keep the saved passphrase</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="413"/>
        <source>私钥口令（无口令留空）</source>
        <translation>Private key passphrase (leave empty if none)</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="415"/>
        <source>登录密码</source>
        <translation>Login password</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="415"/>
        <source>留空保留已保存的密码</source>
        <translation>Leave empty to keep the saved password</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="420"/>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="422"/>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="424"/>
        <source>跟随全局（%1）</source>
        <translation>Follow global (%1)</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="428"/>
        <source>暂未生效：连接流程暂不支持每会话覆盖，当前跟随全局设置</source>
        <translation>Not applied yet: per-session override is not supported by the connect flow; currently follows the global setting</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="432"/>
        <source>关闭</source>
        <translation>Off</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="433"/>
        <source> 秒</source>
        <translation> sec</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="434"/>
        <source>保活间隔，单位秒；0 为关闭</source>
        <translation>Keepalive interval in seconds; 0 disables it</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="436"/>
        <source>SSH Agent</source>
        <translation>SSH Agent</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="437"/>
        <source>公钥文件</source>
        <translation>Public Key File</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="438"/>
        <source>密码</source>
        <translation>Password</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="442"/>
        <source>浏览…</source>
        <translation>Browse…</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="446"/>
        <source>类型</source>
        <translation>Type</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="446"/>
        <source>监听地址</source>
        <translation>Listen Address</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="446"/>
        <source>监听端口</source>
        <translation>Listen Port</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="447"/>
        <source>目标地址</source>
        <translation>Target Address</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="447"/>
        <source>目标端口</source>
        <translation>Target Port</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="452"/>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="488"/>
        <source>本地 -L</source>
        <translation>Local -L</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="453"/>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="489"/>
        <source>远程 -R</source>
        <translation>Remote -R</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="454"/>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="490"/>
        <source>动态 -D</source>
        <translation>Dynamic -D</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="459"/>
        <source>添加</source>
        <translation>Add</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="463"/>
        <source>删除</source>
        <translation>Delete</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="466"/>
        <source>X11 转发</source>
        <translation>X11 Forwarding</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="467"/>
        <source>Windows 端首次使用将下载内建 X server；Linux/macOS 需本机 X server / XQuartz</source>
        <translation>On Windows, the built-in X server is downloaded on first use; on Linux/macOS a local X server / XQuartz is required</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="469"/>
        <source>嵌入标签页显示（实验；否则独立窗口）</source>
        <translation>Embed in tab (experimental; otherwise shown in a separate window)</translation>
    </message>
    <message>
        <location filename="../../src/dialog/ZzSshConfigPage.cpp" line="470"/>
        <source>仅 Windows 生效：X11 桌面嵌入会话标签页内；取消勾选则 X 程序以独立窗口显示</source>
        <translation>Windows only: the X11 desktop is embedded in the session tab; if unchecked, X programs are shown in separate windows</translation>
    </message>
</context>
<context>
    <name>ZzSshTransportAdapter</name>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="160"/>
        <source>创建 shell 通道失败</source>
        <translation>Failed to create shell channel</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="176"/>
        <source>远程 shell 已关闭</source>
        <translation>Remote shell closed</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="254"/>
        <source>转发规则 %1 启动失败：%2</source>
        <translation>Failed to start forwarding rule %1: %2</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="276"/>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="287"/>
        <source>X11 转发已跳过：X server 未启用</source>
        <translation>X11 forwarding skipped: X server not enabled</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="331"/>
        <source>X11 转发不可用：%1</source>
        <translation>X11 forwarding unavailable: %1</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="350"/>
        <source>X11 本地 server 异常：%1</source>
        <translation>Local X11 server error: %1</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="355"/>
        <source>X11 转发不可用：无空闲 display 号</source>
        <translation>X11 forwarding unavailable: no free display number</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="363"/>
        <source>X11 授权写入失败：%1</source>
        <translation>Failed to write X11 authorization: %1</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="399"/>
        <source>X11 转发已跳过：本地授权不可用</source>
        <translation>X11 forwarding skipped: local authorization unavailable</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="410"/>
        <source>X11 转发通道失败：%1</source>
        <translation>X11 forwarding channel failed: %1</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="413"/>
        <source>X11 转发已启用</source>
        <translation>X11 forwarding enabled</translation>
    </message>
    <message>
        <location filename="../../src/transport/ZzSshTransport.cpp" line="417"/>
        <source>X11 转发被服务端拒绝：%1</source>
        <translation>X11 forwarding rejected by server: %1</translation>
    </message>
</context>
<context>
    <name>ZzTabManager</name>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="138"/>
        <source>未知协议「%1」：会话 %2 未打开</source>
        <translation>Unknown protocol &quot;%1&quot;: session %2 was not opened</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="268"/>
        <source>重连失败：协议「%1」未注册</source>
        <translation>Reconnect failed: protocol &quot;%1&quot; is not registered</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="412"/>
        <source>左右分屏	Ctrl+Shift+E</source>
        <translation>Split Left/Right	Ctrl+Shift+E</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="414"/>
        <source>上下分屏	Ctrl+Shift+O</source>
        <translation>Split Up/Down	Ctrl+Shift+O</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="416"/>
        <source>关闭窗格	Ctrl+Shift+W</source>
        <translation>Close Pane	Ctrl+Shift+W</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="419"/>
        <source>重新连接</source>
        <translation>Reconnect</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="421"/>
        <source>关闭标签</source>
        <translation>Close Tab</translation>
    </message>
    <message>
        <location filename="../../src/tab/ZzTabManager.cpp" line="532"/>
        <source>%1 已断开：%2</source>
        <translation>%1 disconnected: %2</translation>
    </message>
</context>
<context>
    <name>ZzTerminalView</name>
    <message>
        <location filename="../../src/terminal/ZzTerminalView.cpp" line="118"/>
        <source>重试</source>
        <translation>Retry</translation>
    </message>
    <message>
        <location filename="../../src/terminal/ZzTerminalView.cpp" line="217"/>
        <source>滚动历史已降级为内存模式：%1</source>
        <translation>Scrollback history degraded to in-memory mode: %1</translation>
    </message>
    <message>
        <location filename="../../src/terminal/ZzTerminalView.cpp" line="223"/>
        <source>滚动历史已降级为温层模式：%1</source>
        <translation>Scrollback history degraded to warm-tier mode: %1</translation>
    </message>
</context>
<context>
    <name>ZzTunnelManager</name>
    <message>
        <location filename="../../src/transport/ZzTunnelManager.cpp" line="31"/>
        <source>创建隧道失败（连接未就绪）</source>
        <translation>Failed to create tunnel (connection not ready)</translation>
    </message>
</context>
<context>
    <name>ZzX11Service</name>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="29"/>
        <location filename="../../src/x11/ZzX11Service.cpp" line="167"/>
        <source>X11 授权写入失败：%1</source>
        <translation>Failed to write X11 authorization: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="94"/>
        <source>X11 转发不可用：%1</source>
        <translation>X11 forwarding unavailable: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="103"/>
        <source>X11 转发已跳过：未检测到本地 X server（$DISPLAY 为空）</source>
        <translation>X11 forwarding skipped: no local X server detected ($DISPLAY is empty)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="108"/>
        <source>X11 转发已跳过：未检测到 XQuartz（/tmp/.X11-unix 不存在）</source>
        <translation>X11 forwarding skipped: XQuartz not detected (/tmp/.X11-unix does not exist)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="151"/>
        <source>X11 转发已跳过：X server 已被禁用</source>
        <translation>X11 forwarding skipped: X server is disabled</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzX11Service.cpp" line="157"/>
        <source>X11 转发不可用：无空闲 display 号</source>
        <translation>X11 forwarding unavailable: no free display number</translation>
    </message>
</context>
<context>
    <name>ZzXServerDownloader</name>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="158"/>
        <source>无法创建下载临时文件：%1</source>
        <translation>Unable to create download temp file: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="190"/>
        <source>安装包下载失败（HTTP %1）</source>
        <translation>Installer download failed (HTTP %1)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="191"/>
        <source>安装包下载失败：%1</source>
        <translation>Installer download failed: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="199"/>
        <source>SHA256 校验失败：期望 %1，实际 %2</source>
        <translation>SHA256 verification failed: expected %1, got %2</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="219"/>
        <source>静默安装超时（%1 分钟）</source>
        <translation>Silent installation timed out (%1 minutes)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="223"/>
        <source>安装程序启动失败：%1</source>
        <translation>Failed to start installer: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="251"/>
        <source>静默安装失败（exit=%1）</source>
        <translation>Silent installation failed (exit=%1)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="257"/>
        <source>安装完成但未找到 %1</source>
        <translation>Installation completed but %1 was not found</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="264"/>
        <source>版本标记写入失败：%1</source>
        <translation>Failed to write version marker: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="273"/>
        <source>旧版安装改名失败，无法换入新版本：%1</source>
        <translation>Failed to rename the old installation; cannot swap in the new version: %1</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerDownloader.cpp" line="279"/>
        <source>新版本换入安装根失败：%1</source>
        <translation>Failed to swap the new version into the installation root: %1</translation>
    </message>
</context>
<context>
    <name>ZzXServerManager</name>
    <message>
        <location filename="../../src/x11/ZzXServerManager.cpp" line="133"/>
        <source>X server 启动失败（display :%1）</source>
        <translation>X server failed to start (display :%1)</translation>
    </message>
    <message>
        <location filename="../../src/x11/ZzXServerManager.cpp" line="148"/>
        <source>X server 非预期退出（退出码 %1）</source>
        <translation>X server exited unexpectedly (exit code %1)</translation>
    </message>
</context>
<context>
    <name>main</name>
    <message>
        <location filename="../../src/main.cpp" line="87"/>
        <source>Terminal</source>
        <translation>Terminal</translation>
    </message>
    <message>
        <location filename="../../src/main.cpp" line="93"/>
        <source>Settings</source>
        <translation>Settings</translation>
    </message>
</context>
</TS>

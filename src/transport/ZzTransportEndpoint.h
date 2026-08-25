#pragma once

#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtCore/QVector>

#include "session/ZzForwardRule.h"

/**
 * @brief 描述一次传输会话所需的全部参数，与协议无关。
 * @note 由应用层从 ZzSessionProfile（计划 03）映射生成；认证凭据不经过本结构，
 *       由具体传输实现通过回调向上层索取（规格 §4.2）。
 */
struct ZzTransportEndpoint final
{
    QString host;       ///< 远程主机地址；localShell 时忽略。
    quint16 port = 22;  ///< 远程端口。
    QString user;       ///< 登录用户名；localShell 时忽略。
    QString terminalType = QStringLiteral("xterm-256color"); ///< TERM 终端类型。
    int cols = 80;      ///< 初始列数。
    int rows = 24;      ///< 初始行数。
    QString keyPath;    ///< 公钥认证的私钥路径（可空）。
    int keepaliveIntervalSeconds = 0; ///< keepalive 间隔（秒），0 表示禁用；localShell 时忽略。
    bool localShell = false; ///< true 表示本地 shell 会话（规格 §七）。
    QString shellProgram;    ///< 本地 shell 可执行路径（可空，空=系统默认）。
    QVector<ZzForwardRule> portForwards; ///< 端口转发规则（规格 §五）；localShell 时为空。
    bool x11Forwarding = false; ///< 是否启用 X11 转发；装配层总是按 profile 显式赋值（ZzTabManager::endpointFor），此默认值仅兜底；localShell 时忽略。
    quintptr x11ParentWindow = 0; ///< X11 嵌入父窗口句柄（0=独立窗口模式；仅 Windows 由 ZzX11Viewport 提供）
    QSize x11InitialSize;      ///< X11 嵌入初始像素尺寸（映射 -screen；独立窗口模式忽略）
};

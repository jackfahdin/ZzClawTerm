#pragma once

#include <QRect>

/**
 * @brief ZzX11Viewport 的私有实现：resize 跟随的 Win32 细节。
 *
 * 仅 Windows 编译有效实现：枚举容器子窗口中类名为 ZzXsrv/x 的 X 窗口
 * 并 SetWindowPos 拉伸至目标矩形；其他平台为空实现。
 */
class ZzX11ViewportPrivate
{
public:
    /**
     * @brief 将容器内类名为 ZzXsrv/x 的 X 窗口拉伸至目标矩形。
     * @param containerHwnd 容器 Win32 句柄（即 ZzX11Viewport::embeddingHandle()）。
     * @param rect 目标矩形（容器客户区坐标，通常取 computeFollowRect 结果）。
     */
    void repositionXChildWindows(quintptr containerHwnd, const QRect &rect) const;
};

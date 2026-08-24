#ifndef ZZX11VIEWPORT_H
#define ZZX11VIEWPORT_H

#include <QWidget>
#include <memory>

class ZzX11ViewportPrivate;

/**
 * @brief X11 嵌入容器：为 ZzXsrv 的 -parent 嵌入提供 Win32 父窗口表面。
 *
 * Windows：以自身 winId 作为 server 边界窗口的父窗口；resize 时枚举
 * 子窗口中的 X 窗口（类名 ZzXsrv/x）并 SetWindowPos 拉伸铺满客户区
 * （零 IPC 跟随；X 逻辑屏幕尺寸不变，放大后边缘可能出现黑边）。
 * 其他平台：编译期直通为空容器（嵌入模式仅 Windows 提供）。
 */
class ZzX11Viewport : public QWidget
{
    Q_OBJECT
public:
    explicit ZzX11Viewport(QWidget *parent = nullptr);
    ~ZzX11Viewport() override;

    /** @brief 供 -parent 参数使用的嵌入句柄（Windows）；其他平台为 0。 */
    quintptr embeddingHandle() const;

    /** @brief ZzXsrv 边界窗口的 Win32 类名（品牌化识别串）。 */
    static QString x11WindowClassName();

    /** @brief resize 跟随的目标矩形（抽离可测）：铺满容器客户区。 */
    static QRect computeFollowRect(const QSize &containerSize);

protected:
    void resizeEvent(QResizeEvent *event) override;

private:
    std::unique_ptr<ZzX11ViewportPrivate> d_ptr;
    Q_DECLARE_PRIVATE(ZzX11Viewport)
    Q_DISABLE_COPY_MOVE(ZzX11Viewport)
};

#endif // ZZX11VIEWPORT_H

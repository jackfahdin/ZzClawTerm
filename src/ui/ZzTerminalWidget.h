/**
 * @file ZzTerminalWidget.h
 * @brief 终端渲染宿主控件 (QPainter 原型实现)。
 *
 * 负责将 ZzTerminalCore 的屏幕缓冲区绘制到屏幕, 并将键盘输入转换为
 * 字节流上报。MVP 阶段使用 QPainter; 后续可替换为 Qt RHI GPU 渲染。
 */

#ifndef ZZ_TERMINAL_WIDGET_H
#define ZZ_TERMINAL_WIDGET_H

#include <QWidget>

#include <memory>

#include "business/ZzColorScheme.h"

namespace ZzCore {
namespace Terminal {
class ZzTerminalCore;
}
}  // namespace ZzCore

namespace ZzUi {

class ZzTerminalWidgetPrivate;

/**
 * @brief 终端渲染控件。
 */
class ZzTerminalWidget : public QWidget
{
    Q_OBJECT

public:
    explicit ZzTerminalWidget(QWidget* parent = nullptr);
    ~ZzTerminalWidget() override;

    /** @brief 绑定终端核心 (不取得所有权)。 */
    void setTerminalCore(ZzCore::Terminal::ZzTerminalCore* core);

    /** @brief 设置配色方案。 */
    void setColorScheme(const ZzBusiness::ZzColorScheme& scheme);

    /** @brief 设置等宽字体并重算单元尺寸。 */
    void setTerminalFont(const QFont& font);

    /** @brief 当前视口可容纳的列数与行数。 */
    QSize viewportCells() const;

signals:
    /** @brief 用户键盘输入对应的字节流, 应写入 PTY。 */
    void keyInput(const QByteArray& data);

    /** @brief 视口尺寸变化 (新的列数、行数)。 */
    void viewportResized(int cols, int rows);

protected:
    void paintEvent(QPaintEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

private:
    std::unique_ptr<ZzTerminalWidgetPrivate> d_ptr;
};

}  // namespace ZzUi

#endif  // ZZ_TERMINAL_WIDGET_H

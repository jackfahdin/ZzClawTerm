/**
 * @file ZzTerminalCore.h
 * @brief 终端核心引擎: 整合 VT 解析与文本缓冲。
 *
 * 接收来自 PTY/网络的字节流, 解析后更新屏幕缓冲区, 并以信号
 * 通知 UI 重绘。滚出屏幕的行以 lineArchived 信号上报给日志引擎。
 */

#ifndef ZZ_TERMINAL_CORE_H
#define ZZ_TERMINAL_CORE_H

#include <QObject>
#include <QSize>

#include <memory>

namespace ZzCore {
namespace Terminal {

class ZzTextBuffer;
class ZzTerminalCorePrivate;

/**
 * @brief 终端核心引擎。
 */
class ZzTerminalCore : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造终端核心。
     * @param cols 初始列数。
     * @param rows 初始行数。
     * @param parent 父对象。
     */
    explicit ZzTerminalCore(int cols = 80, int rows = 24, QObject* parent = nullptr);
    ~ZzTerminalCore() override;

    /**
     * @brief 处理输入字节流 (含转义序列)。
     */
    void processInput(const QByteArray& data);

    /**
     * @brief 调整终端尺寸。
     */
    void resize(int cols, int rows);

    /** @brief 当前尺寸 (列, 行)。 */
    QSize size() const;

    /** @brief 只读访问文本缓冲区 (供渲染器使用)。 */
    const ZzTextBuffer& buffer() const;

signals:
    /** @brief 屏幕内容发生变化, UI 应重绘。 */
    void contentChanged();

    /** @brief 光标移动。 */
    void cursorMoved(int row, int col);

    /** @brief 一行滚出屏幕, 应归档到日志引擎。 */
    void lineArchived(const QString& text);

private:
    std::unique_ptr<ZzTerminalCorePrivate> d_ptr;
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_TERMINAL_CORE_H

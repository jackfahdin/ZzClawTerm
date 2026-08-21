#pragma once

#include <QtCore/QList>
#include <QtCore/QPointer>
#include <QtWidgets/QWidget>

class QSplitter;
class ZzTerminalView;

/**
 * @brief 标签内分屏容器：QSplitter 嵌套树，叶子一律为 ZzTerminalView。
 *
 * 交互约定：分屏以当前焦点窗格为锚点插入新窗格；关闭窗格后自动收拢
 * 只剩单子的非根分割器；焦点经 QApplication::focusChanged 跟踪，
 * focusPane() 按几何方向在窗格间移动焦点（键盘导航用）。
 */
class ZzSplitContainer : public QWidget
{
    Q_OBJECT
public:
    /** @brief 焦点移动方向（Ctrl+Shift+方向键导航用）。 */
    enum class FocusDirection { Left, Right, Up, Down };

    explicit ZzSplitContainer(QWidget *parent = nullptr);

    /** @brief 放入首个窗格（容器为空时调用；分屏请用 splitFocused）。 */
    void addInitialView(ZzTerminalView *view);

    /**
     * @brief 以焦点窗格为锚点分屏：新窗格插入锚点之后。
     * @param orientation 分割方向：Horizontal=左右并排，Vertical=上下排列。
     * @param newView 新窗格（本容器接管所有权）。
     */
    void splitFocused(Qt::Orientation orientation, ZzTerminalView *newView);

    /**
     * @brief 关闭并销毁指定窗格；收拢单子分割器；
     *        最后一个窗格关闭时发射 emptied（不销毁容器自身）。
     */
    void closeView(ZzTerminalView *view);

    /** @brief 按几何方向把焦点移到相邻窗格（无相邻窗格时不动）。 */
    void focusPane(FocusDirection direction);

    /** @brief 当前焦点窗格（无焦点记录时回落到首个窗格，空容器返回 nullptr）。 */
    [[nodiscard]] ZzTerminalView *focusedView() const;

    /** @brief 全部叶子窗格（分割树先根序）。 */
    [[nodiscard]] QList<ZzTerminalView *> views() const;

    /** @brief 窗格数。 */
    [[nodiscard]] int paneCount() const;

    /** @brief 指定视图是否为本容器叶子。 */
    [[nodiscard]] bool containsView(ZzTerminalView *view) const;

signals:
    /** @brief 焦点窗格变化（ZzTabManager 据此刷新状态栏）。 */
    void focusedViewChanged(ZzTerminalView *view);
    /** @brief 窗格即将销毁（ZzTabManager 据此清理 profile/隧道计数表并关闭传输）。 */
    void paneClosing(ZzTerminalView *view);
    /** @brief 最后一个窗格已关闭（ZzTabManager 据此关闭整个标签）。 */
    void emptied();

private:
    /** @brief 收拢从 parent 向上只剩单子的非根分割器（closeView 内调用）。 */
    void collapseSingleChildSplitters(QSplitter *parent);

    QSplitter *m_root = nullptr;               ///< 根分割器（方向随首次分屏调整）
    QPointer<ZzTerminalView> m_focused;        ///< 最近焦点窗格（可空）
};

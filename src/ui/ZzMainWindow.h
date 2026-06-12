/**
 * @file ZzMainWindow.h
 * @brief 应用主窗口: 标签页、分屏、主题、搜索的容器。
 */

#ifndef ZZ_MAIN_WINDOW_H
#define ZZ_MAIN_WINDOW_H

#include <QMainWindow>

#include <memory>

class QShowEvent;

namespace ZzUi {

class ZzMainWindowPrivate;

/**
 * @brief 主窗口。
 */
class ZzMainWindow : public QMainWindow
{
    Q_OBJECT

public:
    explicit ZzMainWindow(QWidget* parent = nullptr);
    ~ZzMainWindow() override;

protected:
    void showEvent(QShowEvent* event) override;

private slots:
    /** @brief 新建终端标签页。 */
    void onNewTab();
    /** @brief 关闭指定标签页。 */
    void onCloseTab(int index);
    /** @brief 将当前标签页水平分屏。 */
    void onSplitHorizontally();
    /** @brief 切换搜索栏。 */
    void onToggleSearch();
    /** @brief 执行搜索。 */
    void onSearch(const QString& pattern);

private:
    std::unique_ptr<ZzMainWindowPrivate> d_ptr;
};

}  // namespace ZzUi

#endif  // ZZ_MAIN_WINDOW_H

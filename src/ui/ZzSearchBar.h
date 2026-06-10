/**
 * @file ZzSearchBar.h
 * @brief 终端搜索栏控件。
 */

#ifndef ZZ_SEARCH_BAR_H
#define ZZ_SEARCH_BAR_H

#include <QWidget>

#include <memory>

namespace ZzUi {

class ZzSearchBarPrivate;

/**
 * @brief 搜索栏: 输入关键词触发搜索, 显示结果计数。
 */
class ZzSearchBar : public QWidget
{
    Q_OBJECT

public:
    explicit ZzSearchBar(QWidget* parent = nullptr);
    ~ZzSearchBar() override;

    /** @brief 显示搜索栏并聚焦输入框。 */
    void activate();

    /** @brief 设置结果计数提示。 */
    void setResultCount(int count);

signals:
    /** @brief 用户请求搜索。 */
    void searchRequested(const QString& pattern);

    /** @brief 搜索栏关闭。 */
    void closed();

private:
    std::unique_ptr<ZzSearchBarPrivate> d_ptr;
};

}  // namespace ZzUi

#endif  // ZZ_SEARCH_BAR_H

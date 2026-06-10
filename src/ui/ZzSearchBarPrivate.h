/**
 * @file ZzSearchBarPrivate.h
 * @brief ZzSearchBar 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_SEARCH_BAR_PRIVATE_H
#define ZZ_SEARCH_BAR_PRIVATE_H

class QLineEdit;
class QLabel;

namespace ZzUi {

/**
 * @brief 搜索栏内部子控件。
 */
class ZzSearchBarPrivate
{
public:
    QLineEdit* input = nullptr;   ///< 关键词输入框
    QLabel* countLabel = nullptr; ///< 结果计数标签
};

}  // namespace ZzUi

#endif  // ZZ_SEARCH_BAR_PRIVATE_H

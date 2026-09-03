#pragma once

#include <QtWidgets/QWidget>

#include "session/ZzSessionProfile.h"

class QLineEdit;
class QStackedWidget;
class QTreeView;

/**
 * @brief 本地 Shell 协议配置页：左侧树形导航 + 右侧 QStackedWidget 配置页。
 *
 * 与 ZzSshConfigPage 同接口、同结构，仅一个「常规」节点（栈页索引 0）：
 * 名称 / 分组路径 / Shell 程序。契约约定：local 会话的 shell 程序路径
 * 存于 profile.host 字段，留空表示系统默认 shell。
 */
class ZzLocalShellConfigPage : public QWidget
{
    Q_OBJECT
public:
    explicit ZzLocalShellConfigPage(QWidget *parent = nullptr);

    /** @brief 按 profile 回填全部字段（新建传默认值亦可）。 */
    void setProfile(const ZzSessionProfile &profile);
    /** @brief 把表单内容写回 profile（不触碰 id 与两个 credentialId）。 */
    void applyTo(ZzSessionProfile &profile) const;
    /**
     * @brief 校验表单。
     * @param error 输出错误文案（已翻译）。
     * @param pageIndex 输出出错字段所在栈页索引（用于窗口层聚焦）。
     * @return 全部合法返回 true。
     */
    [[nodiscard]] bool validateInputs(QString *error, int *pageIndex) const;

    /** @brief 切换到指定栈页并选中对应树节点（校验失败聚焦用）。 */
    void focusPage(int pageIndex);

protected:
    /** @brief LanguageChange 时重设全部文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 重设全部用户可见文本（构造与 LanguageChange 共用单一路径）。 */
    void retranslateUi();

    QTreeView *m_navTree;    ///< 仅一个「常规」节点
    QStackedWidget *m_stack; ///< 一页：名称 / 分组路径 / Shell 程序
    QLineEdit *m_nameEdit;
    QLineEdit *m_groupEdit;
    QLineEdit *m_shellEdit;  ///< local 会话的 shell 路径（留空=系统默认）
};

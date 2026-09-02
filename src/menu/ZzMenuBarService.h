#pragma once

#include <QtCore/QObject>
#include <QtCore/QStringList>

class QAction;
class QActionGroup;
class QEvent;
class QMenu;
class QMenuBar;
class ZzAppSettings;
class ZzLanguageManager;

/**
 * @brief 标题栏菜单装配：会话/视图/帮助三菜单挂入框架标题栏 menuBar。
 *
 * 菜单动作全部意图转发——会话新建→ZzAppShell::requestNewSession()，
 * 主题→ZzThemeController::setMode，终端主题→ZzAppSettings::setColorScheme，
 * 语言→ZzLanguageManager::apply，帮助→关于对话框/日志目录/仓库链接。
 * 文本集中在 retranslate()，LanguageChange 时重刷。
 *
 * 构造传入 QMenuBar（生产=ZzFluentTitleBar::menuBar()，测试=普通 QMenuBar），
 * 本服务不感知标题栏，天然可离屏测试。
 */
class ZzMenuBarService : public QObject
{
    Q_OBJECT
public:
    ZzMenuBarService(QMenuBar *menuBar,
                     ZzAppSettings *settings,
                     ZzLanguageManager *languages,
                     QObject *shellActions,   ///< ZzAppShell：菜单动作槽宿主
                     QObject *parent = nullptr);

    /** @brief 重设全部菜单/动作文本（构造后调用一次；LanguageChange 自动调）。 */
    void retranslate();

    // ---- 测试观察口 ----
    [[nodiscard]] QMenu *sessionMenu() const;
    [[nodiscard]] QMenu *viewMenu() const;
    [[nodiscard]] QMenu *helpMenu() const;
    [[nodiscard]] QMenu *themeSubMenu() const;
    [[nodiscard]] QMenu *terminalThemeSubMenu() const;
    [[nodiscard]] QMenu *languageSubMenu() const;
    [[nodiscard]] QActionGroup *themeGroup() const;
    [[nodiscard]] QActionGroup *terminalThemeGroup() const;
    [[nodiscard]] QActionGroup *languageGroup() const;

protected:
    /** @brief LanguageChange 时 retranslate()。 */
    bool event(QEvent *event) override;

private:
    void buildMenus();
    void syncCheckedStates();
    static QStringList curatedColorSchemes();  ///< 精选配色清单（校验存在性后过滤）

    QMenuBar *m_menuBar;          ///< 非拥有
    ZzAppSettings *m_settings;    ///< 非拥有
    ZzLanguageManager *m_langs;   ///< 非拥有
    QObject *m_shell;             ///< 非拥有（槽宿主）
    QMenu *m_sessionMenu = nullptr;
    QMenu *m_viewMenu = nullptr;
    QMenu *m_helpMenu = nullptr;
    QMenu *m_themeSub = nullptr;
    QMenu *m_terminalThemeSub = nullptr;
    QMenu *m_languageSub = nullptr;
    QActionGroup *m_themeGroup = nullptr;
    QActionGroup *m_terminalThemeGroup = nullptr;
    QActionGroup *m_languageGroup = nullptr;
    QAction *m_moreSchemesAction = nullptr;
};

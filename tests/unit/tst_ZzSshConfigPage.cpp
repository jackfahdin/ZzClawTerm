#include <QtTest>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QLineEdit>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTableWidget>
#include <QTreeView>

#include "dialog/ZzSshConfigPage.h"

/**
 * @brief ZzSshConfigPage 单元测试：树导航、页面切换与 profile 往返。
 */
class tst_ZzSshConfigPage : public QObject
{
    Q_OBJECT

    static ZzSessionProfile sampleProfile()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.groupPath = QStringLiteral("生产/Web");
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("10.0.0.1");
        profile.port = 2222;
        profile.userName = QStringLiteral("root");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/u/.ssh/id_ed25519");
        profile.terminalType = QStringLiteral("xterm-256color");
        profile.encoding = QStringLiteral("UTF-8");
        profile.colorSchemeName = QStringLiteral("Linux");
        profile.keepAliveIntervalSeconds = 30;
        profile.x11Forwarding = true;
        profile.x11EmbedMode = true;
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
        };
        return profile;
    }

private slots:
    /** @brief 树有六个节点，切节点同步切 QStackedWidget 页。 */
    void treeSwitchesStackedPage()
    {
        ZzSshConfigPage page;
        auto *tree = page.findChild<QTreeView *>(QStringLiteral("sshNavTree"));
        auto *stack = page.findChild<QStackedWidget *>();
        QVERIFY(tree && stack);
        QCOMPARE(tree->model()->rowCount(), 6);
        QCOMPARE(stack->count(), 6);
        const QModelIndex index = tree->model()->index(3, 0); // 端口转发
        tree->setCurrentIndex(index);
        QCOMPARE(stack->currentIndex(), 3);
    }

    /** @brief setProfile 回填全部字段；applyTo 无损写回。 */
    void profileRoundTrip()
    {
        ZzSshConfigPage page;
        const ZzSessionProfile original = sampleProfile();
        page.setProfile(original);

        // 抽查回填：转发表一行、X11 两勾选、认证方式与端口
        auto *table = page.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        auto *x11 = page.findChild<QCheckBox *>(QStringLiteral("x11CheckBox"));
        auto *embed = page.findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"));
        auto *auth = page.findChild<QComboBox *>(QStringLiteral("authCombo"));
        QVERIFY(table && x11 && embed && auth);
        QCOMPARE(table->rowCount(), 1);
        QVERIFY(x11->isChecked());
        QVERIFY(embed->isChecked());
        QCOMPARE(auth->currentData().toInt(),
                 static_cast<int>(ZzAuthMethod::PrivateKey));

        ZzSessionProfile out;
        // applyTo 只覆盖 SSH 页负责的字段；id/credentialId 由窗口层保留
        out.id = original.id;
        page.applyTo(out);
        QCOMPARE(out.name, original.name);
        QCOMPARE(out.groupPath, original.groupPath);
        QCOMPARE(out.protocol, QStringLiteral("ssh"));
        QCOMPARE(out.host, original.host);
        QCOMPARE(out.port, original.port);
        QCOMPARE(out.userName, original.userName);
        QCOMPARE(out.authMethod, original.authMethod);
        QCOMPARE(out.privateKeyPath, original.privateKeyPath);
        QCOMPARE(out.terminalType, original.terminalType);
        QCOMPARE(out.encoding, original.encoding);
        QCOMPARE(out.colorSchemeName, original.colorSchemeName);
        QCOMPARE(out.keepAliveIntervalSeconds, original.keepAliveIntervalSeconds);
        QCOMPARE(out.x11Forwarding, original.x11Forwarding);
        QCOMPARE(out.x11EmbedMode, original.x11EmbedMode);
        QCOMPARE(out.portForwards.size(), 1);
        QCOMPARE(out.portForwards.at(0).targetHost, QStringLiteral("db.internal"));
    }

    /** @brief 主机为空时 validateInputs 报错并给出出错页索引（连接=1）。 */
    void validateReportsEmptyHost()
    {
        ZzSshConfigPage page;
        page.setProfile(sampleProfile());
        auto *hostEdit = page.findChild<QLineEdit *>(QStringLiteral("hostEdit"));
        QVERIFY(hostEdit);
        hostEdit->clear();
        QString error;
        int pageIndex = -1;
        QVERIFY(!page.validateInputs(&error, &pageIndex));
        QVERIFY(!error.isEmpty());
        QCOMPARE(pageIndex, 1);
    }

    /** @brief 非法转发规则（监听端口 0）报错并定位到端口转发页（索引 3）。 */
    void validateReportsInvalidForwardRule()
    {
        ZzSshConfigPage page;
        page.setProfile(sampleProfile());
        auto *table = page.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        table->item(0, 2)->setText(QStringLiteral("0"));
        QString error;
        int pageIndex = -1;
        QVERIFY(!page.validateInputs(&error, &pageIndex));
        QCOMPARE(pageIndex, 3);
    }

    /** @brief 空 terminalType/encoding/colorSchemeName 选中「跟随全局」首项并写回空串。 */
    void emptyFieldsFollowGlobal()
    {
        ZzSshConfigPage page;
        ZzSessionProfile profile = sampleProfile();
        profile.terminalType.clear();
        profile.encoding.clear();
        profile.colorSchemeName.clear();
        page.setProfile(profile);

        auto *termType =
            page.findChild<QComboBox *>(QStringLiteral("terminalTypeCombo"));
        auto *encoding = page.findChild<QComboBox *>(QStringLiteral("encodingCombo"));
        auto *scheme = page.findChild<QComboBox *>(QStringLiteral("colorSchemeCombo"));
        QVERIFY(termType && encoding && scheme);
        QCOMPARE(termType->currentIndex(), 0);
        QCOMPARE(encoding->currentIndex(), 0);
        QCOMPARE(scheme->currentIndex(), 0);
        // 首项 itemData 为空串（跟随全局标记）
        QVERIFY(termType->itemData(0).toString().isEmpty());
        QVERIFY(encoding->itemData(0).toString().isEmpty());
        QVERIFY(scheme->itemData(0).toString().isEmpty());

        ZzSessionProfile out;
        page.applyTo(out);
        QVERIFY(out.terminalType.isEmpty());
        QVERIFY(out.encoding.isEmpty());
        QVERIFY(out.colorSchemeName.isEmpty());
    }

    /** @brief 编码与配色 combo 置灰占位（连接流程暂不消费每会话覆盖）。 */
    void encodingAndColorSchemeDisabled()
    {
        ZzSshConfigPage page;
        auto *encoding = page.findChild<QComboBox *>(QStringLiteral("encodingCombo"));
        auto *scheme = page.findChild<QComboBox *>(QStringLiteral("colorSchemeCombo"));
        QVERIFY(encoding && scheme);
        QVERIFY(!encoding->isEnabled());
        QVERIFY(!scheme->isEnabled());
        QVERIFY(!encoding->toolTip().isEmpty());
        QVERIFY(!scheme->toolTip().isEmpty());
    }

    /** @brief setCredentialHints 按已存凭据切换占位提示，LanguageChange 后提示不丢。 */
    void credentialHintsSwitchPlaceholders()
    {
        ZzSshConfigPage page;
        auto *pwd = page.findChild<QLineEdit *>(QStringLiteral("passwordEdit"));
        auto *pass = page.findChild<QLineEdit *>(QStringLiteral("keyPassphraseEdit"));
        QVERIFY(pwd && pass);
        QCOMPARE(pwd->placeholderText(), QStringLiteral("登录密码"));
        QCOMPARE(pass->placeholderText(), QStringLiteral("私钥口令（无口令留空）"));

        page.setCredentialHints(true, true);
        QCOMPARE(pwd->placeholderText(), QStringLiteral("留空保留已保存的密码"));
        QCOMPARE(pass->placeholderText(), QStringLiteral("留空保留已保存的口令"));

        // retranslateUi 重跑（语言切换）后提示保留
        QEvent langEvent(QEvent::LanguageChange);
        QApplication::sendEvent(&page, &langEvent);
        QCOMPARE(pwd->placeholderText(), QStringLiteral("留空保留已保存的密码"));
        QCOMPARE(pass->placeholderText(), QStringLiteral("留空保留已保存的口令"));

        page.setCredentialHints(false, false);
        QCOMPARE(pwd->placeholderText(), QStringLiteral("登录密码"));
        QCOMPARE(pass->placeholderText(), QStringLiteral("私钥口令（无口令留空）"));
    }
};

QTEST_MAIN(tst_ZzSshConfigPage)
#include "tst_ZzSshConfigPage.moc"

#include <QtTest>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QJsonDocument>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QTableWidget>
#include <QTimer>

#include "panel/ZzSessionEditDialog.h"
#include "session/ZzCredentialStore.h"

/**
 * @brief ZzSessionEditDialog 端口转发规则表单元测试（规格 §五）。
 */
class tst_ZzSessionEditDialog : public QObject
{
    Q_OBJECT

    /** @brief 临时目录凭据库（对话框构造需要；本组用例不涉及密码）。 */
    std::unique_ptr<ZzCredentialStore> makeStore()
    {
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-editdlg-%1").arg(QCoreApplication::applicationPid()));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        return std::make_unique<ZzCredentialStore>(dir + QStringLiteral("/credentials.dat"));
    }

    /** @brief 造带两条规则的 profile。 */
    static ZzSessionProfile profileWithRules()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.host = QStringLiteral("10.0.0.1");
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        return profile;
    }

    /** @brief 点击对话框 OK 按钮。 */
    static void clickOk(QDialog &dlg)
    {
        auto *buttons = dlg.findChild<QDialogButtonBox *>();
        QVERIFY(buttons);
        buttons->button(QDialogButtonBox::Ok)->click();
    }

    /** @brief 安排自动关闭即将弹出的模态 QMessageBox（校验失败提示会阻塞嵌套事件循环）。 */
    static void autoDismissMessageBox()
    {
        // QMessageBox::warning 走嵌套事件循环，singleShot 在其中触发并关闭弹窗
        QTimer::singleShot(0, [] {
            if (auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget())) {
                box->accept();
            }
        });
    }

private slots:
    /** @brief 构造时表格按 profile.portForwards 填充。 */
    void ctorPopulatesTableFromProfile()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        QVERIFY(table);
        QCOMPARE(table->rowCount(), 2);
        QCOMPARE(table->item(0, 1)->text(), QStringLiteral("127.0.0.1"));
        QCOMPARE(table->item(0, 2)->text(), QStringLiteral("13306"));
        QCOMPARE(table->item(0, 3)->text(), QStringLiteral("db.internal"));
        QCOMPARE(table->item(0, 4)->text(), QStringLiteral("3306"));
        // 类型列为下拉框
        auto *typeCombo = qobject_cast<QComboBox *>(table->cellWidget(0, 0));
        QVERIFY(typeCombo);
        QCOMPARE(typeCombo->currentData().toInt(),
                 static_cast<int>(ZzForwardRule::Type::Local));
    }

    /** @brief 合法规则：accept 后 profile().portForwards 与表格一致。 */
    void acceptSavesValidRules()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.result(), QDialog::Accepted);
        QCOMPARE(dlg.profile().portForwards.size(), 2);
        QCOMPARE(dlg.profile().portForwards.at(0).targetHost, QStringLiteral("db.internal"));
    }

    /** @brief 非法规则（监听端口 0）禁止保存：accept 被拒绝，profile 不变。 */
    void invalidRuleBlocksAccept()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        table->item(0, 2)->setText(QStringLiteral("0")); // 非法监听端口

        autoDismissMessageBox(); // 校验失败的 QMessageBox 为模态，自动关闭防挂起
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 0); // 未 finished = accept 被拒绝
        QCOMPARE(dlg.profile().portForwards.size(), 2); // 工作副本未写回
    }

    /** @brief 重复规则（同 type+listenHost+listenPort）禁止保存。 */
    void duplicateRuleBlocksAccept()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        // 把第二行改成与第一行同三元组
        auto *typeCombo = qobject_cast<QComboBox *>(table->cellWidget(1, 0));
        typeCombo->setCurrentIndex(0); // Local
        table->item(1, 1)->setText(QStringLiteral("127.0.0.1"));
        table->item(1, 2)->setText(QStringLiteral("13306"));

        autoDismissMessageBox();
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 0);
    }

    /** @brief 添加/删除按钮增减行。 */
    void addRemoveButtonsWork()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), ZzSessionProfile{});
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        auto *addBtn = dlg.findChild<QPushButton *>(QStringLiteral("addForwardButton"));
        auto *removeBtn = dlg.findChild<QPushButton *>(QStringLiteral("removeForwardButton"));
        QVERIFY(table && addBtn && removeBtn);
        QCOMPARE(table->rowCount(), 0);

        addBtn->click();
        QCOMPARE(table->rowCount(), 1); // 默认新增一行 Local 规则
        table->selectRow(0);
        removeBtn->click();
        QCOMPARE(table->rowCount(), 0);
    }

    /** @brief 公钥认证 + 私钥口令：accept 后口令写入凭据库，profile 仅留引用。 */
    void privateKeyPassphraseSavedToStore()
    {
        auto store = makeStore();
        QVERIFY(store->initialize(QStringLiteral("pw")));

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("密钥机");
        profile.host = QStringLiteral("10.0.0.2");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/u/.ssh/id_ed25519");
        ZzSessionEditDialog dlg(store.get(), profile);
        auto *passEdit = dlg.findChild<QLineEdit *>(QStringLiteral("keyPassphraseEdit"));
        QVERIFY(passEdit);
        passEdit->setText(QStringLiteral("pass-口令"));

        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        const QUuid passphraseId = dlg.profile().keyPassphraseCredentialId;
        QVERIFY(!passphraseId.isNull());
        // 口令密文入凭据库，可按引用取回
        QCOMPARE(store->credential(passphraseId).value(), QStringLiteral("pass-口令"));
        // profile 序列化不含口令明文（仅有 id 引用）
        QVERIFY(!QString::fromUtf8(QJsonDocument(dlg.profile().toJson()).toJson())
                     .contains(QStringLiteral("pass-口令")));
    }

    /** @brief 已有口令引用且未输入新口令：accept 保留原引用。 */
    void emptyPassphraseKeepsOriginalReference()
    {
        auto store = makeStore();
        QVERIFY(store->initialize(QStringLiteral("pw")));

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("密钥机");
        profile.host = QStringLiteral("10.0.0.2");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/u/.ssh/id_ed25519");
        profile.keyPassphraseCredentialId =
            store->addCredential(QStringLiteral("密钥机 私钥口令"), QStringLiteral("old-pass"));
        QVERIFY(!profile.keyPassphraseCredentialId.isNull());

        ZzSessionEditDialog dlg(store.get(), profile);
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.profile().keyPassphraseCredentialId, profile.keyPassphraseCredentialId);
    }

    /** @brief 对话框暴露 x11CheckBox，加载/保存与 profile.x11Forwarding 一致。 */
    void dialogExposesX11Checkbox()
    {
        auto store = makeStore();

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("图形机");
        profile.host = QStringLiteral("10.0.0.3");
        profile.x11Forwarding = true;

        ZzSessionEditDialog dlg(store.get(), profile);
        auto *check = dlg.findChild<QCheckBox *>(QStringLiteral("x11CheckBox"));
        QVERIFY(check);
        QVERIFY(check->isChecked()); // 构造时按 profile 加载
        QVERIFY(!check->toolTip().isEmpty());

        // 取消勾选后保存，profile 随之为 false
        check->setChecked(false);
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.profile().x11Forwarding, false);

        // 反向：false profile 加载未勾选，勾选后保存为 true
        ZzSessionProfile plain;
        plain.id = QUuid::createUuid();
        plain.name = QStringLiteral("终端机");
        plain.host = QStringLiteral("10.0.0.4");
        ZzSessionEditDialog dlg2(store.get(), plain);
        auto *check2 = dlg2.findChild<QCheckBox *>(QStringLiteral("x11CheckBox"));
        QVERIFY(check2);
        QVERIFY(!check2->isChecked());
        check2->setChecked(true);
        clickOk(dlg2);
        QCOMPARE(dlg2.profile().x11Forwarding, true);
    }

    /** @brief 对话框暴露 x11EmbedCheckBox，加载/保存与 profile.x11EmbedMode 一致。 */
    void dialogExposesX11EmbedCheckbox()
    {
        auto store = makeStore();

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("图形机");
        profile.host = QStringLiteral("10.0.0.3");
        profile.x11EmbedMode = false; // 独立窗口

        ZzSessionEditDialog dlg(store.get(), profile);
        auto *check = dlg.findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"));
        QVERIFY(check);
        QVERIFY(!check->isChecked()); // 构造时按 profile 加载
        QVERIFY(!check->toolTip().isEmpty());

        // 勾选后保存，profile 随之为 true
        check->setChecked(true);
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.profile().x11EmbedMode, true);

        // 反向：缺省 profile（true）加载为勾选，取消勾选后保存为 false
        ZzSessionProfile plain;
        plain.id = QUuid::createUuid();
        plain.name = QStringLiteral("终端机");
        plain.host = QStringLiteral("10.0.0.4");
        ZzSessionEditDialog dlg2(store.get(), plain);
        auto *check2 = dlg2.findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"));
        QVERIFY(check2);
        QVERIFY(check2->isChecked()); // x11EmbedMode 缺省 true
        check2->setChecked(false);
        clickOk(dlg2);
        QCOMPARE(dlg2.profile().x11EmbedMode, false);
    }

    /** @brief 密码认证下私钥口令引用被清空，旧口令凭据一并删除（不留孤儿条目）。 */
    void passwordAuthClearsKeyPassphraseReference()
    {
        auto store = makeStore();
        QVERIFY(store->initialize(QStringLiteral("pw")));

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("密钥机");
        profile.host = QStringLiteral("10.0.0.2");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/u/.ssh/id_ed25519");
        profile.keyPassphraseCredentialId =
            store->addCredential(QStringLiteral("密钥机 私钥口令"), QStringLiteral("old-pass"));

        ZzSessionEditDialog dlg(store.get(), profile);
        auto *authCombo = dlg.findChild<QComboBox *>(QStringLiteral("authCombo"));
        QVERIFY(authCombo);
        authCombo->setCurrentIndex(
            authCombo->findData(static_cast<int>(ZzAuthMethod::Password)));
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QVERIFY(dlg.profile().keyPassphraseCredentialId.isNull());
        // 旧口令凭据已从凭据库删除
        QVERIFY(!store->credential(profile.keyPassphraseCredentialId).has_value());
    }
};

QTEST_MAIN(tst_ZzSessionEditDialog)
#include "tst_ZzSessionEditDialog.moc"

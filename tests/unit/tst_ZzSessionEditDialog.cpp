#include <QtTest>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
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
};

QTEST_MAIN(tst_ZzSessionEditDialog)
#include "tst_ZzSessionEditDialog.moc"

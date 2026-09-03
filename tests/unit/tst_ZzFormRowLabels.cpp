#include <QtTest>
#include <QDir>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QStandardItemModel>
#include <QTabWidget>
#include <QTreeView>

#include "dialog/ZzLocalShellConfigPage.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "dialog/ZzSessionConfigWindow.h"
#include "dialog/ZzSshConfigPage.h"
#include "session/ZzCredentialStore.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"

/**
 * @brief 表单行标签回归测试（2026-09-03 新建会话窗口「没有名字」缺陷）。
 *
 * 根因：QFormLayout::addRow(const QString &, QWidget *) 在标签文本为空串时
 * 不创建标签部件，后续 retranslateUi 经 labelForField() 反查永远拿到
 * nullptr，行标签永久缺失。本组用例钉死契约：凡是有字段部件的表单行，
 * 必须存在非空文本的 QLabel。
 */
class tst_ZzFormRowLabels : public QObject
{
    Q_OBJECT

    /** @brief 断言 root 下所有 QFormLayout 的字段行都有非空标签。 */
    static void assertAllFieldRowsHaveLabels(QWidget *root)
    {
        const auto forms = root->findChildren<QFormLayout *>();
        QVERIFY(!forms.isEmpty());
        for (auto *form : forms) {
            for (int row = 0; row < form->rowCount(); ++row) {
                QLayoutItem *fieldItem =
                    form->itemAt(row, QFormLayout::FieldRole);
                QWidget *field = fieldItem ? fieldItem->widget() : nullptr;
                if (!field) {
                    continue;
                }
                // 跨列整行部件（提示语/按钮盒）在 itemAt(FieldRole) 中也会
                // 出现，用 getWidgetPosition 的角色判定排除，它们无标签属正常
                int foundRow = -1;
                QFormLayout::ItemRole role = QFormLayout::FieldRole;
                form->getWidgetPosition(field, &foundRow, &role);
                if (role != QFormLayout::FieldRole) {
                    continue;
                }
                QLayoutItem *labelItem =
                    form->itemAt(row, QFormLayout::LabelRole);
                auto *label = qobject_cast<QLabel *>(
                    labelItem ? labelItem->widget() : nullptr);
                QVERIFY2(label,
                         qPrintable(QStringLiteral(
                             "表单 %1 第 %2 行缺标签部件")
                                        .arg(form->parentWidget()
                                                 ->objectName())
                                        .arg(row)));
                QVERIFY2(!label->text().isEmpty(),
                         qPrintable(QStringLiteral(
                             "表单 %1 第 %2 行标签文本为空")
                                        .arg(form->parentWidget()
                                                 ->objectName())
                                        .arg(row)));
            }
        }
    }

    /** @brief 临时目录凭据库（主密码框构造需要）。 */
    static std::unique_ptr<ZzCredentialStore> makeStore()
    {
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-rowlabels-%1")
                          .arg(QCoreApplication::applicationPid()));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        return std::make_unique<ZzCredentialStore>(
            dir + QStringLiteral("/credentials.dat"));
    }

private slots:
    /** @brief SSH 配置页：五个表单页的全部字段行均有非空标签。 */
    void sshPageRowsHaveLabels()
    {
        ZzSshConfigPage page;
        assertAllFieldRowsHaveLabels(&page);
    }

    /** @brief 本地 Shell 配置页：常规页全部字段行均有非空标签。 */
    void localShellPageRowsHaveLabels()
    {
        ZzLocalShellConfigPage page;
        assertAllFieldRowsHaveLabels(&page);
    }

    /** @brief 主密码对话框（首次设置形态）：密码/确认行均有非空标签。 */
    void masterPasswordDialogRowsHaveLabels()
    {
        auto store = makeStore();
        ZzMasterPasswordDialog dlg(store.get());
        assertAllFieldRowsHaveLabels(&dlg);
    }

    /** @brief 设置页：全部字段行均有非空标签（同一根因的存量受损面）。 */
    void settingsPageRowsHaveLabels()
    {
        const QString iniPath = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-rowlabels-%1.ini")
                          .arg(QCoreApplication::applicationPid()));
        QFile::remove(iniPath);
        ZzAppSettings settings(iniPath);
        ZzSettingsPage page(&settings);
        assertAllFieldRowsHaveLabels(&page);
    }

    /** @brief 窗口级：协议 tab 文本与两页树节点文本非空（用户报告措辞覆盖）。 */
    void windowTabsAndTreesHaveNames()
    {
        auto store = makeStore();
        ZzSessionProfile profile; // 新建：id 为空
        ZzSessionConfigWindow dlg(store.get(), profile, QString());

        auto *tabs =
            dlg.findChild<QTabWidget *>(QStringLiteral("protocolTabWidget"));
        QVERIFY(tabs);
        QCOMPARE(tabs->count(), 2);
        for (int i = 0; i < tabs->count(); ++i) {
            QVERIFY2(!tabs->tabText(i).isEmpty(),
                     qPrintable(QStringLiteral("tab %1 文本为空").arg(i)));
        }
        for (const auto treeName :
             {QStringLiteral("sshNavTree"), QStringLiteral("localNavTree")}) {
            auto *tree = dlg.findChild<QTreeView *>(treeName);
            QVERIFY(tree);
            auto *model = qobject_cast<QStandardItemModel *>(tree->model());
            QVERIFY(model);
            QVERIFY(model->rowCount() > 0);
            for (int i = 0; i < model->rowCount(); ++i) {
                QVERIFY2(!model->item(i, 0)->text().isEmpty(),
                         qPrintable(treeName +
                                    QStringLiteral(" 节点 %1 文本为空")
                                        .arg(i)));
            }
        }
    }
};

QTEST_MAIN(tst_ZzFormRowLabels)
#include "tst_ZzFormRowLabels.moc"

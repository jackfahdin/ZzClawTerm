#include <QtTest/QtTest>

#include <QtWidgets/QTreeView>

#include "panel/ZzSessionPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"

/**
 * @brief 验证会话面板：树形分组、双击发连接请求、增删改后树刷新（规格 §七）。
 */
class tst_ZzSessionPanel : public QObject
{
    Q_OBJECT
private:
    QString m_dir;

    /** @brief 造一条会话记录并加入模型，返回模型分配的 id。 */
    static QUuid addProfile(ZzSessionModel &model, const QString &name,
                            const QString &groupPath)
    {
        ZzSessionProfile profile;
        profile.name = name;
        profile.groupPath = groupPath;
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("example.com");
        profile.userName = QStringLiteral("root");
        return model.addSession(profile);
    }

    /** @brief ZzSessionModel 的 id 是 QUuid，面板观察口统一用无括号字符串。 */
    static QString idString(const QUuid &id)
    {
        return id.toString(QUuid::WithoutBraces);
    }

private slots:
    void init()
    {
        qRegisterMetaType<ZzSessionProfile>(); // connectRequested 信号参数
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-panel-test"));
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
    }

    void treeGroupsByPath()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        addProfile(model, QStringLiteral("Web1"),
                   QStringLiteral("生产环境/Web 服务器"));
        addProfile(model, QStringLiteral("DB1"),
                   QStringLiteral("生产环境/数据库"));
        addProfile(model, QStringLiteral("本机"), QString());

        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QCOMPARE(panel.panelId(), QStringLiteral("sessions"));
        QCOMPARE(panel.panelWidget(), static_cast<QWidget *>(&panel));

        // 树：生产环境（含 Web 服务器、数据库两个子组）+ 未分组"本机"
        QCOMPARE(panel.visibleGroupCount(), 1);          // 顶层分组数
        QCOMPARE(panel.visibleSessionCount(), 3);        // 会话叶子总数
    }

    void doubleClickEmitsConnectRequest()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        const QUuid idA = addProfile(model, QStringLiteral("Web1"),
                                     QStringLiteral("生产环境"));
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QSignalSpy spy(&panel, &ZzSessionPanel::connectRequested);

        panel.triggerConnect(idString(idA)); // 等价于双击该会话项
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.first().at(0).value<ZzSessionProfile>().id, idA);

        // 双击分组项不触发
        panel.triggerConnect(QString()); // 分组项无 profile id
        QCOMPARE(spy.count(), 1);
    }

    void modelChangeRebuildsTree()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QCOMPARE(panel.visibleSessionCount(), 0);

        const QUuid idA = addProfile(model, QStringLiteral("Web1"), QString());
        QCOMPARE(panel.visibleSessionCount(), 1);

        model.removeSession(idA);
        QCOMPARE(panel.visibleSessionCount(), 0);
    }

    void deleteViaActionRemovesFromModel()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        const QUuid idA = addProfile(model, QStringLiteral("Web1"), QString());
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        panel.triggerDelete(idString(idA)); // 等价于右键→删除
        QVERIFY(!model.session(idA).has_value());
        QCOMPARE(panel.visibleSessionCount(), 0);
    }
};

QTEST_MAIN(tst_ZzSessionPanel)
#include "tst_ZzSessionPanel.moc"

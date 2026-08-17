#include <QtTest>
#include <QFile>
#include <QFileInfo>
#include <QSignalSpy>
#include <QTemporaryDir>

#include "ZzSessionModel.h"

namespace {

/**
 * @brief 构造一个用于测试的会话档案。
 * @param name 会话名称。
 * @param groupPath 分组路径。
 */
ZzSessionProfile makeProfile(const QString &name, const QString &groupPath)
{
    ZzSessionProfile profile;
    profile.name = name;
    profile.groupPath = groupPath;
    profile.host = QStringLiteral("192.168.1.10");
    profile.port = 22;
    profile.userName = QStringLiteral("root");
    profile.authMethod = ZzAuthMethod::Password;
    profile.credentialId = QUuid::createUuid();
    return profile;
}

} // namespace

/**
 * @brief ZzSessionModel 增删改查与持久化单元测试。
 */
class ZzSessionModelTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 添加会话后可通过返回的 id 查询到，且触发 sessionsChanged 信号。 */
    void addAndQuery()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);

        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"),
                                                      QStringLiteral("生产环境/Web 服务器")));
        QVERIFY(!id.isNull());
        QCOMPARE(model.allSessions().size(), 1);
        QCOMPARE(spy.count(), 1);

        const std::optional<ZzSessionProfile> fetched = model.session(id);
        QVERIFY(fetched.has_value());
        QVERIFY(fetched->id == id);
        QCOMPARE(fetched->name, QStringLiteral("Web-01"));
        QCOMPARE(fetched->groupPath, QStringLiteral("生产环境/Web 服务器"));
    }

    /** @brief 调用方自带 id 时尊重该 id；id 冲突时拒绝添加。 */
    void addDuplicateIdRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

        const QUuid fixed = QUuid::createUuid();
        ZzSessionProfile first = makeProfile(QStringLiteral("A"), QString());
        first.id = fixed;
        QVERIFY(model.addSession(first) == fixed);

        ZzSessionProfile second = makeProfile(QStringLiteral("B"), QString());
        second.id = fixed;
        QVERIFY(model.addSession(second).isNull());
        QVERIFY(!model.errorString().isEmpty());
        QCOMPARE(model.allSessions().size(), 1);
    }

    /** @brief 更新已存在的会话；更新不存在的 id 返回 false。 */
    void updateSession()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(!id.isNull());

        ZzSessionProfile updated = model.session(id).value();
        updated.port = 2222;
        updated.host = QStringLiteral("10.0.0.99");
        QVERIFY(model.updateSession(updated));
        QCOMPARE(model.session(id)->port, quint16(2222));
        QCOMPARE(model.session(id)->host, QStringLiteral("10.0.0.99"));

        ZzSessionProfile ghost = updated;
        ghost.id = QUuid::createUuid();
        QVERIFY(!model.updateSession(ghost));
    }

    /** @brief 删除已存在的会话；重复删除返回 false。 */
    void removeSession()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);

        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(model.removeSession(id));
        QVERIFY(model.allSessions().isEmpty());
        QVERIFY(!model.session(id).has_value());
        QCOMPARE(spy.count(), 2); // add + remove

        QVERIFY(!model.removeSession(id));
        QVERIFY(!model.errorString().isEmpty());
    }

    /** @brief 保存后由新模型实例加载，内容必须逐条相等（序列化往返）。 */
    void persistenceRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("sessions.json"));

        ZzSessionModel writer(path);
        writer.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境/Web 服务器")));
        writer.addSession(makeProfile(QStringLiteral("DB-01"), QStringLiteral("生产环境/数据库")));
        writer.addSession(makeProfile(QStringLiteral("本地"), QString()));
        QVERIFY(writer.save());

        ZzSessionModel reader(path);
        QVERIFY(reader.load());
        QVERIFY(reader.allSessions() == writer.allSessions());
    }

    /** @brief 文件不存在（首次启动）时 load 返回 true 且模型为空。 */
    void loadMissingFileIsEmptyModel()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QVERIFY(model.load());
        QVERIFY(model.allSessions().isEmpty());
    }

    /** @brief 文件存在但内容不是合法 JSON 对象时 load 返回 false 并给出错误信息。 */
    void loadCorruptFileFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("sessions.json"));
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("this is not json");
        file.close();

        ZzSessionModel model(path);
        QVERIFY(!model.load());
        QVERIFY(!model.errorString().isEmpty());
    }

    /** @brief save 在目录不存在时自动创建目录。 */
    void saveCreatesParentDirectory()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("nested/deep/sessions.json"));

        ZzSessionModel model(path);
        model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(model.save());
        QVERIFY(QFileInfo::exists(path));
    }
};

QTEST_GUILESS_MAIN(ZzSessionModelTest)

#include "ZzSessionModelTest.moc"

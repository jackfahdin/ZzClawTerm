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

    /** @brief 未知/缺失 version 字段的会话文件必须报错，不得静默解析。 */
    void loadUnknownVersionFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString futurePath =
            dir.filePath(QStringLiteral("sessions-future.json"));
        const QString missingPath =
            dir.filePath(QStringLiteral("sessions-missing.json"));

        // 未来版本号
        QFile futureFile(futurePath);
        QVERIFY(futureFile.open(QIODevice::WriteOnly));
        futureFile.write(R"({"version": 99, "sessions": []})");
        futureFile.close();
        ZzSessionModel futureModel(futurePath);
        QVERIFY(!futureModel.load());
        QVERIFY(futureModel.errorString().contains(QStringLiteral("版本")));

        // 缺失 version 字段
        QFile missingFile(missingPath);
        QVERIFY(missingFile.open(QIODevice::WriteOnly));
        missingFile.write(R"({"sessions": []})");
        missingFile.close();
        ZzSessionModel missingModel(missingPath);
        QVERIFY(!missingModel.load());
        QVERIFY(!missingModel.errorString().isEmpty());
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

    /** @brief allGroupPaths 返回去重并排序的全部分组路径（含嵌套路径全量，不含空分组）。 */
    void allGroupPaths();

    /** @brief sessionsInGroup 只返回直接位于该分组的会话，不含子分组。 */
    void sessionsInGroup();

    /** @brief 重命名分组时同路径及子路径前缀一并改写。 */
    void renameGroup();

    /** @brief 非法重命名（空路径、同名、重命名为自身子分组）被拒绝。 */
    void renameGroupInvalidRejected();

    /** @brief 删除分组时级联删除该分组及其子分组下的全部会话。 */
    void removeGroup();
};

void ZzSessionModelTest::allGroupPaths()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("Web-02"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("DB-01"), QStringLiteral("生产环境/数据库")));
    model.addSession(makeProfile(QStringLiteral("Test-01"), QStringLiteral("测试环境")));
    model.addSession(makeProfile(QStringLiteral("本地"), QString()));

    const QStringList groups = model.allGroupPaths();
    QCOMPARE(groups.size(), 3);
    QVERIFY(groups.contains(QStringLiteral("生产环境/Web 服务器")));
    QVERIFY(groups.contains(QStringLiteral("生产环境/数据库")));
    QVERIFY(groups.contains(QStringLiteral("测试环境")));
}

void ZzSessionModelTest::sessionsInGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境")));
    model.addSession(makeProfile(QStringLiteral("Web-02"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("Web-03"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("本地"), QString()));

    QCOMPARE(model.sessionsInGroup(QStringLiteral("生产环境")).size(), 1);
    QCOMPARE(model.sessionsInGroup(QStringLiteral("生产环境/Web 服务器")).size(), 2);
    QVERIFY(model.sessionsInGroup(QStringLiteral("不存在")).isEmpty());
    QVERIFY(model.sessionsInGroup(QString()).isEmpty());
}

void ZzSessionModelTest::renameGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    const QUuid idTop = model.addSession(makeProfile(QStringLiteral("Prod"), QStringLiteral("生产")));
    const QUuid idSub = model.addSession(makeProfile(QStringLiteral("ProdWeb"), QStringLiteral("生产/Web")));
    const QUuid idOther = model.addSession(makeProfile(QStringLiteral("ProdMirror"), QStringLiteral("生产环境-镜像")));
    const QUuid idNone = model.addSession(makeProfile(QStringLiteral("Local"), QString()));

    // spy 在 addSession 之后创建，只统计 renameGroup 触发的信号
    QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);
    QVERIFY(model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产环境")));
    QCOMPARE(spy.count(), 1);

    QCOMPARE(model.session(idTop)->groupPath, QStringLiteral("生产环境"));
    QCOMPARE(model.session(idSub)->groupPath, QStringLiteral("生产环境/Web"));
    // "生产环境-镜像" 不是 "生产" 的子路径（前缀边界必须是 '/'），不得被误改
    QCOMPARE(model.session(idOther)->groupPath, QStringLiteral("生产环境-镜像"));
    QVERIFY(model.session(idNone)->groupPath.isEmpty());
}

void ZzSessionModelTest::renameGroupInvalidRejected()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产/Web")));

    QVERIFY(!model.renameGroup(QString(), QStringLiteral("新分组")));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QString()));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产")));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产/Web/子分组")));

    // 无匹配分组时视为幂等成功
    QVERIFY(model.renameGroup(QStringLiteral("不存在"), QStringLiteral("任意")));
}

void ZzSessionModelTest::removeGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Prod"), QStringLiteral("生产")));
    model.addSession(makeProfile(QStringLiteral("ProdWeb"), QStringLiteral("生产/Web")));
    const QUuid idOther = model.addSession(makeProfile(QStringLiteral("Test"), QStringLiteral("测试")));

    QVERIFY(model.removeGroup(QStringLiteral("生产")));
    QCOMPARE(model.allSessions().size(), 1);
    QVERIFY(model.allSessions().first().id == idOther);

    // 空路径拒绝；无匹配返回 false
    QVERIFY(!model.removeGroup(QString()));
    QVERIFY(!model.removeGroup(QStringLiteral("不存在")));
}

QTEST_GUILESS_MAIN(ZzSessionModelTest)

#include "ZzSessionModelTest.moc"

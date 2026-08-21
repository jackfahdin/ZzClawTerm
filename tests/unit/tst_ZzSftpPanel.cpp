#include <QtTest/QtTest>

#include "ZzMockSftpOps.h"
#include "ZzMockTransport.h"
#include "panel/ZzSftpPanel.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "transport/ZzTransportRegistry.h"

namespace {

/** @brief 造一个目录条目。 */
ZzSftpFileInfo makeDirEntry(const QString &name)
{
    ZzSftpFileInfo info;
    info.name = name;
    info.permissions = LIBSSH2_SFTP_S_IFDIR | 0755;
    info.mtime = 1700000000;
    return info;
}

/** @brief 造一个文件条目。 */
ZzSftpFileInfo makeFileEntry(const QString &name, qint64 size)
{
    ZzSftpFileInfo info;
    info.name = name;
    info.size = size;
    info.permissions = LIBSSH2_SFTP_S_IFREG | 0644;
    info.mtime = 1700000000;
    return info;
}

} // namespace

/**
 * @brief 验证 SFTP 面板：会话跟随/不可用提示、目录浏览与导航、
 *        文件操作请求配对刷新、传输队列状态流转（mock 隔离网络）。
 */
class tst_ZzSftpPanel : public QObject
{
    Q_OBJECT
private:
    /** @brief 附着已打开的 mock 并等根目录列举完成。 */
    static void attachOpenedMock(ZzSftpPanel &panel, ZzMockSftpOps &mock,
                                 const QList<ZzSftpFileInfo> &entries)
    {
        mock.dirReply = entries;
        panel.attachOpsForTesting(&mock); // mock 不由面板持有
        mock.simulateOpened();
        QTRY_COMPARE(panel.currentPath(), QStringLiteral("/"));
        QTRY_COMPARE(panel.visibleEntryCount(), entries.size());
    }

private slots:
    void panelIdentity()
    {
        ZzSftpPanel panel;
        QCOMPARE(panel.panelId(), QStringLiteral("sftp"));
        QCOMPARE(panel.panelTitle(), QStringLiteral("SFTP"));
        QCOMPARE(panel.panelWidget(), static_cast<QWidget *>(&panel));
        QVERIFY(!panel.opsAttached());
        QCOMPARE(panel.statusText(), QStringLiteral("无活动会话"));
    }

    void localSessionShowsUnavailable()
    {
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
        ZzTabManager tabs;
        ZzSftpPanel panel;
        panel.setTabManager(&tabs);
        QCOMPARE(panel.statusText(), QStringLiteral("无活动会话"));

        ZzSessionProfile profile;
        profile.name = QStringLiteral("本机");
        profile.protocol = QStringLiteral("mock");
        tabs.openSession(profile); // mock 传输非 SSH 适配器 → 面板提示不可用
        QVERIFY(!panel.opsAttached());
        QVERIFY(panel.statusText().contains(QStringLiteral("不可用")));

        tabs.closeTab(0); // 末标签关闭 → 回到无活动会话
        QCoreApplication::processEvents();
        QCOMPARE(panel.statusText(), QStringLiteral("无活动会话"));
        ZzTransportRegistry::instance().clear();
    }

    void openedListsRootAndNavigates()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock,
                         {makeDirEntry(QStringLiteral("docs")),
                          makeFileEntry(QStringLiteral("a.txt"), 123)});
        QVERIFY(panel.opsAttached());
        QVERIFY(mock.listedPaths.contains(QStringLiteral("/")));

        // 双击目录等价 navigateTo；返回上级等价 triggerUp
        panel.navigateTo(QStringLiteral("/docs"));
        QTRY_COMPARE(panel.currentPath(), QStringLiteral("/docs"));
        QCOMPARE(mock.listedPaths.last(), QStringLiteral("/docs"));

        panel.triggerUp();
        QTRY_COMPARE(panel.currentPath(), QStringLiteral("/"));
        QCOMPARE(mock.listedPaths.last(), QStringLiteral("/"));

        // 路径栏刷新等价 navigateTo；刷新当前目录重发 listDir
        panel.triggerRefresh();
        QTRY_VERIFY(mock.listedPaths.count(QStringLiteral("/")) >= 2);
    }

    void makeDirRemoveRenameRefreshOnFinish()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        const int baseListCalls = mock.listedPaths.size();

        panel.requestMakeDir(QStringLiteral("newdir"));
        QCOMPARE(mock.madeDirs, QStringList{QStringLiteral("/newdir")});
        // operationFinished → 自动刷新当前目录
        QTRY_COMPARE(mock.listedPaths.size(), baseListCalls + 1);

        panel.requestRemove(QStringLiteral("/a.txt"), false);
        QCOMPARE(mock.removedFiles, QStringList{QStringLiteral("/a.txt")});
        QTRY_COMPARE(mock.listedPaths.size(), baseListCalls + 2);

        panel.requestRemove(QStringLiteral("/docs"), true);
        QCOMPARE(mock.removedDirs, QStringList{QStringLiteral("/docs")});
        QTRY_COMPARE(mock.listedPaths.size(), baseListCalls + 3);

        panel.requestRename(QStringLiteral("/a.txt"), QStringLiteral("b.txt"));
        QCOMPARE(mock.renamed.size(), 1);
        QCOMPARE(mock.renamed.first().first, QStringLiteral("/a.txt"));
        QCOMPARE(mock.renamed.first().second, QStringLiteral("/b.txt"));
        QTRY_COMPARE(mock.listedPaths.size(), baseListCalls + 4);
    }

    void operationErrorShowsStatusAndMessage()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        QSignalSpy spy(&panel, &ZzSftpPanel::statusMessage);
        mock.opErrorCode = 500;
        mock.opErrorMessage = QStringLiteral("permission denied");

        panel.requestMakeDir(QStringLiteral("x"));
        QTRY_VERIFY(panel.statusText().contains(
            QStringLiteral("permission denied")));
        QCOMPARE(spy.count(), 1);
    }

    void uploadsTrackTransferQueue()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        const quint64 firstId = mock.nextReqId;

        panel.startUploads({QStringLiteral("/tmp/a.bin"),
                            QStringLiteral("/tmp/b.bin")});
        QCOMPARE(mock.uploaded.size(), 2);
        QCOMPARE(mock.uploaded.at(0).second, QStringLiteral("/a.bin"));
        QCOMPARE(mock.uploaded.at(1).second, QStringLiteral("/b.bin"));
        QCOMPARE(panel.transferRowCount(), 2);
        QCOMPARE(panel.transferStatusText(firstId), QStringLiteral("进行中"));

        mock.simulateProgress(firstId, 50, 100);
        QCOMPARE(panel.transferStatusText(firstId), QStringLiteral("进行中"));
        mock.simulateTransferFinished(firstId);
        QCOMPARE(panel.transferStatusText(firstId), QStringLiteral("完成"));

        mock.simulateTransferError(firstId + 1, 1, QStringLiteral("io error"));
        QVERIFY(panel.transferStatusText(firstId + 1)
                    .contains(QStringLiteral("io error")));
    }

    void downloadRecordsRequest()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        panel.startDownload(QStringLiteral("/var/log/app.log"),
                            QStringLiteral("/home/zz/app.log"));
        QCOMPARE(mock.downloaded.size(), 1);
        QCOMPARE(mock.downloaded.first().first,
                 QStringLiteral("/var/log/app.log"));
        QCOMPARE(mock.downloaded.first().second,
                 QStringLiteral("/home/zz/app.log"));
        QCOMPARE(panel.transferRowCount(), 1);
    }

    void cancelTransferMarksCancelled()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        const quint64 id = mock.nextReqId;
        panel.startUploads({QStringLiteral("/tmp/a.bin")});

        panel.requestCancelTransfer(id);
        QCOMPARE(mock.cancelledTransfers, QList<quint64>{id});
        // 取消结局为 transferError（Cancelled），状态标记"已取消"
        mock.simulateTransferError(id, 2, QStringLiteral("cancelled"));
        QCOMPARE(panel.transferStatusText(id), QStringLiteral("已取消"));
    }

    void closedDetachesOps()
    {
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock,
                         {makeFileEntry(QStringLiteral("a.txt"), 1)});
        mock.simulateClosed();
        QVERIFY(!panel.opsAttached());
        QVERIFY(panel.statusText().contains(QStringLiteral("已关闭")));
        // 会话关闭后操作退化为提示，不再产生请求
        panel.navigateTo(QStringLiteral("/etc"));
        QCOMPARE(mock.listedPaths.count(QStringLiteral("/etc")), 0);
    }

    void closedReevaluatesBinding()
    {
        // 修复项 7：onClosed 后延迟重评估绑定，无可用视图时提示回到
        // "无活动会话"（修复前面板永久停在"SFTP 会话已关闭"）
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        mock.simulateClosed();
        QVERIFY(!panel.opsAttached());
        QTRY_COMPARE(panel.statusText(), QStringLiteral("无活动会话"));
    }

    void listDirErrorUnlocksLoading()
    {
        // 修复项 1：listDir 失败（operationError 携带 listReqId）必须解除
        // 加载态（修复前 m_loading 永久为 true，工具栏永久禁用）
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        mock.listDirErrorCode = 4;
        mock.listDirErrorMessage = QStringLiteral("permission denied");
        panel.attachOpsForTesting(&mock);
        mock.simulateOpened(); // → navigateTo("/") → listDir → operationError
        QTRY_VERIFY(panel.statusText().contains(
            QStringLiteral("permission denied")));
        QTRY_VERIFY(!panel.isLoading());

        // 恢复后导航可继续（面板未被卡死）
        mock.listDirErrorCode = 0;
        mock.dirReply = {makeFileEntry(QStringLiteral("ok.txt"), 1)};
        panel.navigateTo(QStringLiteral("/"));
        QTRY_COMPARE(panel.visibleEntryCount(), 1);
        QVERIFY(!panel.isLoading());
    }

    void batchFillStaleCallbackDiscarded()
    {
        // 修复项 2：分批填充的延迟回调按代际作废——填充未跑完时重新导航，
        // 过期回调不得继续填充旧队列、不得提前结束新一轮加载状态
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        mock.autoReplyDir = false;
        panel.attachOpsForTesting(&mock);
        mock.simulateOpened(); // navigateTo("/") → listDir id=1（无自动应答）
        QVERIFY(panel.isLoading());

        QList<ZzSftpFileInfo> big;
        for (int i = 0; i < 1200; ++i) {
            big.append(makeFileEntry(
                QStringLiteral("old-%1").arg(i, 4, 10, QLatin1Char('0')), i));
        }
        mock.simulateDirListed(1, big); // 同步填首批 500，其余 700 排队延迟回调

        // 立刻重新导航：代际递增，旧延迟回调作废
        mock.autoReplyDir = true;
        mock.dirReply = {makeFileEntry(QStringLiteral("fresh.txt"), 1)};
        panel.navigateTo(QStringLiteral("/other"));
        QTRY_COMPARE(panel.currentPath(), QStringLiteral("/other"));
        QTRY_COMPARE(panel.visibleEntryCount(), 1);

        QCoreApplication::processEvents(); // 放过期回调跑一遍
        QCOMPARE(panel.visibleEntryCount(), 1); // 旧回调不得继续填充
        QVERIFY(!panel.isLoading());
        QVERIFY(panel.statusText().contains(QStringLiteral("1")));
    }

    void transferHistoryCapped()
    {
        // 修复项 8：已结束传输行修剪到上限 100（进行中行永不移除）
        ZzMockSftpOps mock;
        ZzSftpPanel panel;
        attachOpenedMock(panel, mock, {});
        for (int i = 0; i < 105; ++i) {
            const quint64 id = mock.nextReqId;
            panel.startUploads({QStringLiteral("/tmp/f%1.bin").arg(i)});
            mock.simulateTransferFinished(id);
        }
        QCOMPARE(panel.transferRowCount(), 100);
        // 最早的行已被修剪；最近的行可查
        QCOMPARE(panel.transferStatusText(mock.nextReqId - 1),
                 QStringLiteral("完成"));
        // 换绑清空请求哈希后，历史行观察口仍可用（行内角色数据回退）
        mock.simulateClosed();
        QCOMPARE(panel.transferStatusText(mock.nextReqId - 1),
                 QStringLiteral("完成"));
    }

    void largeDirFillsInBatches()
    {
        ZzSftpPanel panel;
        QList<ZzSftpFileInfo> entries;
        entries.reserve(1200); // 超过单批 500，验证分批填充路径
        for (int i = 0; i < 1200; ++i) {
            entries.append(makeFileEntry(
                QStringLiteral("f%1.txt").arg(i, 4, 10, QLatin1Char('0')), i));
        }
        ZzMockSftpOps mock;
        attachOpenedMock(panel, mock, entries);
        QCOMPARE(panel.visibleEntryCount(), 1200);
        QVERIFY(panel.statusText().contains(QStringLiteral("1200")));
        // "." / ".." 被过滤
        ZzMockSftpOps mock2;
        mock2.dirReply = {makeFileEntry(QStringLiteral("."), 0),
                          makeFileEntry(QStringLiteral(".."), 0),
                          makeFileEntry(QStringLiteral("real.txt"), 1)};
        ZzSftpPanel panel2;
        panel2.attachOpsForTesting(&mock2);
        mock2.simulateOpened();
        QTRY_COMPARE(panel2.visibleEntryCount(), 1);
    }

    void selectionEnablesDownloadForFilesOnly()
    {
        ZzSftpPanel panel;
        ZzMockSftpOps mock;
        attachOpenedMock(panel, mock,
                         {makeDirEntry(QStringLiteral("docs")),
                          makeFileEntry(QStringLiteral("a.txt"), 7)});
        // 选中文件后可经 selectEntry 定位（下载/删除/重命名以此取选中路径）
        QVERIFY(panel.selectEntry(QStringLiteral("a.txt")));
        QVERIFY(panel.selectEntry(QStringLiteral("docs")));
        QVERIFY(!panel.selectEntry(QStringLiteral("missing")));
    }
};

QTEST_MAIN(tst_ZzSftpPanel)
#include "tst_ZzSftpPanel.moc"

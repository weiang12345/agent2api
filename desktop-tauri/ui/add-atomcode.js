/* AtomCode 添加配置；由 add-provider-forms.js 统一渲染并绑定。 */
(() => {
  window.wbAtomCodeAddForm = {
    provider: 'atomcode',
    label: 'AtomCode',
    desktop: false,
    webLogin: {
      noteHtml: '打开 AtomGit 授权页并完成登录，网关会自动取回 OAuth 凭证、领取 CodingPlan 并同步模型目录。'
        + '全程不需要本地 AtomCode 客户端。',
      button: '打开 AtomCode 授权页',
      busyText: '等待 AtomCode 授权完成…',
      modes: [
        {
          value: 'embedded',
          label: '内嵌窗口（推荐）',
          hint: '将打开 AtomGit 授权页，确认后自动加入账号列表。关掉窗口即取消等待',
        },
        {
          value: 'external',
          label: '系统浏览器',
          hint: '将用系统默认浏览器打开 AtomGit 授权页（会复用浏览器里已登录的 AtomGit 账号）；确认后自动加入账号列表',
        },
      ],
    },
    manualTitle: '填写 OAuth 凭证',
    manualNote: '优先使用上方网页登录。这里仅用于恢复已有账号：需要 AtomCode OAuth 的 accessToken、refreshToken 与 userId，三者缺一不可。',
    fields: [
      { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 AtomCode OAuth access token' },
      { key: 'refreshToken', label: 'refreshToken', rows: 2, placeholder: '粘贴 AtomCode OAuth refresh token' },
      { key: 'userId', label: 'userId', placeholder: 'AtomGit 用户 ID' },
      { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用 AtomGit 昵称' },
    ],
  };
})();

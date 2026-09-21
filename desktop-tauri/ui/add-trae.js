/* Trae 添加配置；由 add-provider-forms.js 统一渲染并绑定。 */
(() => {
  window.wbTraeAddForm = {
    provider: 'trae',
    label: 'Trae',
    desktop: false,
    webLogin: {
      noteHtml: '打开 Trae 授权页并完成登录，网关会自动取回凭证并保存账号。'
        + '当前接入的是国内版 Trae SOLO，不需要本地 Trae 客户端。',
      button: '打开 Trae 授权页',
      busyText: '等待 Trae 授权完成…',
      modes: [
        {
          value: 'embedded',
          label: '内嵌窗口（推荐）',
          hint: '将打开 Trae 授权页，确认后自动加入账号列表。关掉窗口即取消等待',
        },
        {
          value: 'external',
          label: '系统浏览器',
          hint: '将用系统默认浏览器打开 Trae 授权页；确认后自动加入账号列表',
        },
      ],
    },
    manualTitle: '填写 OAuth 凭证',
    manualNote: '优先使用上方网页登录。这里仅用于恢复已有账号：需要 accessToken、refreshToken、userId、machineId、deviceId。',
    fields: [
      { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 Trae access token' },
      { key: 'refreshToken', label: 'refreshToken', rows: 2, placeholder: '粘贴 Trae refresh token' },
      { key: 'userId', label: 'userId', placeholder: 'Trae 用户 ID' },
      { key: 'machineId', label: 'machineId', placeholder: '登录时生成的 machineId' },
      { key: 'deviceId', label: 'deviceId', placeholder: '登录时生成的 deviceId' },
      { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用 Trae 昵称' },
    ],
  };
})();

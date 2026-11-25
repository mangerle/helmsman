use crate::app::HelmsmanWindow;
use crate::backend;
use gpui_kit::*;

impl HelmsmanWindow {
    /// 弹出系统文件选择器导入本地主题压缩包 (.zip / .tar.gz / .tar)
    pub fn pick_and_install_theme_archive(&mut self, cx: &mut Context<Self>) {
        if self.is_installing_theme {
            return;
        }
        if self.backend.is_demo_mode {
            self.toast = Some((
                false,
                "演示模式无法写入系统主题目录，请先启动 helmsman-daemon".to_string(),
            ));
            cx.notify();
            return;
        }

        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 GRUB 主题压缩包 (.zip / .tar.gz / .tar)".into()),
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };

            let _ = this.update(cx, |win, cx| {
                win.is_installing_theme = true;
                win.toast = Some((true, format!("正在安装主题包: {}...", path.display())));
                cx.notify();
            });

            let path_str = path.to_string_lossy().into_owned();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let installed = backend::install_theme_archive(&path_str, "")?;
                    let themes = backend::list_installed_themes().unwrap_or_default();
                    Ok::<_, String>((installed, themes))
                })
                .await;

            let _ = this.update(cx, |win, cx| {
                win.is_installing_theme = false;
                match result {
                    Ok((installed, themes)) => {
                        win.backend.themes = themes;
                        win.toast = Some((true, format!("主题安装成功: {installed}")));
                    }
                    Err(e) => {
                        win.toast = Some((false, e));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 弹出系统文件选择器选取 GRUB 开机背景壁纸
    pub fn pick_grub_background(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 GRUB 开机背景图片 (.png / .jpg / .tga)".into()),
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |win, cx| {
                let path_str = path.to_string_lossy().into_owned();
                match win.state.set_grub_background_path(Some(&path_str)) {
                    Ok(()) => {
                        win.toast = Some((true, format!("已设置壁纸草稿: {path_str}")));
                    }
                    Err(e) => {
                        win.toast = Some((false, e.to_string()));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 卸载指定名称的已安装 GRUB 主题
    pub fn uninstall_theme(&mut self, theme_name: String, cx: &mut Context<Self>) {
        if self.backend.is_demo_mode {
            self.toast = Some((false, "演示模式不支持卸载系统主题".to_string()));
            cx.notify();
            return;
        }
        match backend::remove_theme(&theme_name) {
            Ok(()) => {
                self.backend.themes.retain(|t| t.name != theme_name);
                self.toast = Some((true, format!("已卸载主题 '{theme_name}'")));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 在线下载 GRUB 主题归档并交由特权守护进程安装
    pub fn download_and_install_online_theme(
        &mut self,
        url: String,
        theme_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.is_installing_theme {
            return;
        }
        if self.backend.is_demo_mode {
            self.toast = Some((
                false,
                "演示模式无法将下载主题写入系统目录，请先启动 helmsman-daemon".to_string(),
            ));
            cx.notify();
            return;
        }

        let trimmed = url.trim().to_string();
        if trimmed.is_empty() {
            self.toast = Some((false, "请先输入或选择主题下载链接".to_string()));
            cx.notify();
            return;
        }

        self.is_installing_theme = true;
        self.toast = Some((true, format!("正在下载并安装主题: {trimmed} ...")));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let installed = backend::download_and_install_theme(&trimmed, &theme_name)?;
                    let themes = backend::list_installed_themes().unwrap_or_default();
                    Ok::<_, String>((installed, themes))
                })
                .await;

            let _ = this.update(cx, |win, cx| {
                win.is_installing_theme = false;
                match result {
                    Ok((installed, themes)) => {
                        win.backend.themes = themes;
                        win.toast = Some((true, format!("在线主题已安装至: {installed}")));
                    }
                    Err(e) => {
                        win.toast = Some((false, e));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

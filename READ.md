
## 项目介绍

当前的APP是一个类似 warp、finalShell、iterm2(macos) 的 跨平台(Windows/Macos)的桌面应用，核心是一个终端工具，主要面向的是开发者。

主要功能如下：

1. 核心：SSH2终端连接 + 多标签管理 + 配置存储
2. 插件：系统监控(CPU/内存) + 文件浏览 + 软件检测
3. 技术栈：Rust + egui + ssh2 + tokio
  - egui/eframe 统一实现UI，包括终端显示和性能监控
  - ssh2 实现原生SSH连接和终端交互
    - 选择ssh2的原因：
      1. 性能问题
      2. 连接任何终端，可以提前将密码和连接信息 同时一次性传递给终端，建立连接
      3. 能够使用绝大多数的命令，包括vim等，可以编辑终端文件
      4. 能够建立连接后捕获连接后的初始消息（包括 ANSI 转义序列）
  - tokio 实现异步

详细功能如下：

- 核心功能
  - 基础 ssh 功能
    - 主要布局
      - 顶部终端tab区域、显示区域(包含了输入区域)
        - 终端tab区域说明
          - 特殊页面 也就是 终端页面的入口，每增加一个页面，都是进入的这个页面
            - 这里展示 一个 终端列表(数据先本地存储，存储json数据)，用于展示所有填写的终端，双击后方便快速连接，列表 标题为：快速连接，标题右侧 有 清空的按钮，用于删除所有终端，以及一个 添加终端的按钮
            - 编辑||添加 终端页面
              - 默认就是 SSH连接
                - 字段如下：
                  - 名称 主机 端口 备注
                  - 认证类型： 密码、公钥 选择密码就显示密码的 input，公钥是选择文件，点击确定无误后，会将数据添加到 终端列表中，用户需要再次双击 某个列表数据才能进入 开始ssh连接
          - 其他的每一个 页面都是一个终端，最右侧的tab的右侧 会有一个 + 的icon，用于添加
        - 输入区域说明
          - 上方小提示块儿 显示当前 环境+用户名+路径 例如 (base) PS C:\Users\Administrator> 或者 (base) ➜  ~
    - 功能
      - 连接状态管理（断线重连）
      - 会话持久化
      - 需要解决编码问题 中文字符和其他字符
      - 输入时
        - 鼠标捕获后，如果按上下 则需要和ssh交互 获取历史记录
        - 提供自动补全（如输入 conda 提示 install）。
        - 支持多行输入（如脚本执行）。
      - 支持流式输出（如 conda install 的实时进度）。
      - 快捷键功能(该功能后期添加)
- 额外插件
  - 1 当前终端连接的设备 的 设备情况
    - 设备性能展示 如 CPU、内存(交换内存)、磁盘使用率、上下行网速, 展示方式例如：使用折线图（CPU/内存随时间变化）或数字面板（实时百分比）。
    - 支持用户自定义刷新频率（如 1s、5s）以降低资源占用。
  - 2 当前路径的dir列表
  - 3 当前终端连接的设备 的 软件情况
    - php
    - mysql
    - redis
    - docker
    - node
    - conda
    - python
    - nvidia
    - nvidia 的 nvcc
    - ...
    - 补充
      - 未安装时的安装引导
        - 提示安装命令 或 下载连接
      - 支持扩展,允许用户自定义检测软件列表如通过配置文件(后期迭代再实现，这里不做深入)

## 项目实现框架

### 文件结构

#### 📁 应用层 (src/app/)
- **mod.rs** - 应用模块入口，导出TabAppFactory
- **tab_app.rs** - 基于Tab系统的主应用，使用设计模式实现
  - `TabBasedApp` - 主应用结构体
    - `tab_manager: TabManager` - Tab管理器，管理所有Tab的生命周期
  - `TabAppFactory` - 工厂模式，创建应用实例
    - `create_app()` - 创建TabBasedApp实例的工厂方法

#### 📁 配置层 (src/config/)
- **mod.rs** - 配置管理模块，处理应用配置的加载、保存和默认值
  - `AppConfig` - 应用配置结构体
    - `connections: Vec<ConnectionConfig>` - SSH连接配置列表，存储用户添加的所有终端连接
    - `settings: AppSettings` - 应用设置，包含主题、字体等用户偏好
  - `AppSettings` - 应用设置结构体
    - `theme: String` - 主题名称，控制UI外观
    - `font_size: u16` - 字体大小，影响终端和UI文字显示
    - `refresh_interval: u64` - 刷新间隔(ms)，控制系统监控数据更新频率

#### 📁 插件层 (src/plugins/)
- **mod.rs** - 插件系统入口，定义Plugin trait统一接口
  - `Plugin` trait - 插件统一接口
    - `name()` - 获取插件名称，用于UI显示
    - `is_enabled()` - 检查插件是否可用，处理系统兼容性
    - `initialize()` - 插件初始化，准备运行环境
    - `update()` - 更新插件数据，获取最新信息
    - `render_data()` - 渲染插件数据，返回JSON格式供UI显示
- **file_browser.rs** - 文件浏览器插件，浏览远程服务器文件系统
  - `FileBrowser` - 文件浏览器结构体
    - `current_path: PathBuf` - 当前路径，跟踪用户浏览位置
    - `files: Vec<FileInfo>` - 文件列表，缓存当前目录的文件信息
  - `FileInfo` - 文件信息结构体
    - `name: String` - 文件名，显示给用户
    - `is_directory: bool` - 是否为目录，决定图标和操作
    - `size: u64` - 文件大小，显示文件信息
    - `modified: String` - 修改时间，显示文件状态
- **software_detector.rs** - 软件检测插件，检测远程服务器安装的开发工具
  - `SoftwareDetector` - 软件检测器结构体
    - `detected_software: HashMap<String, SoftwareInfo>` - 已检测软件列表
    - `detection_rules: Vec<DetectionRule>` - 检测规则，定义如何检测各种软件
  - `SoftwareInfo` - 软件信息结构体
    - `name: String` - 软件名称，如"Node.js", "Docker"
    - `version: Option<String>` - 软件版本，从命令输出解析
    - `is_installed: bool` - 是否已安装，决定显示状态
    - `install_command: Option<String>` - 安装命令，提供给用户
- **system_monitor.rs** - 系统监控插件，监控远程服务器性能指标
  - `SystemMonitor` - 系统监控器结构体
    - `system: sysinfo::System` - 系统信息获取器，sysinfo库的核心对象
    - `refresh_interval: u64` - 刷新间隔，控制数据更新频率
    - `cpu_usage: f32` - CPU使用率，实时性能指标
    - `memory_info: MemoryInfo` - 内存信息，包含使用量和总量
    - `disk_info: Vec<DiskInfo>` - 磁盘信息列表，多个磁盘的使用情况

#### 📁 SSH/网络层 (src/ssh/)
- **mod.rs** - SSH模块入口，导出SSH2实现
- **ssh2_client.rs** - 基于ssh2库的SSH连接实现，**采用Actor模式+消息传递架构**
  - **架构选择原因**：
    - ✅ **无锁设计** - 避免多线程锁竞争，提高性能和稳定性
    - ✅ **简单清晰** - 一个SSH连接对应一个Actor，职责单一
    - ✅ **内存安全** - 消息传递天然避免数据竞争和并发问题
    - ✅ **易于扩展** - 新功能只需要添加新消息类型
    - ✅ **符合Rust特性** - 利用mpsc::channel的高性能消息传递
  - **Actor架构设计**：
    ```rust
    // SSH Actor - 独占管理一个SSH连接
    struct SshActor {
        connection: Ssh2Connection,  // 只有Actor能操作连接
        message_receiver: Receiver<SshMessage>,
    }
    
    // 消息类型 - 所有操作都通过消息
    enum SshMessage {
        SendCommand(String),    // 发送命令消息
        ReadOutput,            // 读取输出消息
        Disconnect,            // 断开连接消息
    }
    ```
  - `Ssh2Connection` - SSH2连接结构体（被Actor独占管理）
    - `session: ssh2::Session` - SSH2会话，主要的SSH连接对象
    - `channel: ssh2::Channel` - SSH2通道，用于数据交互的虚拟终端
    - `config: ConnectionConfig` - 连接配置，存储主机、用户名、认证信息
    - `tcp_stream: TcpStream` - TCP连接，底层网络通信
    - `is_connected: bool` - 连接状态标志，跟踪SSH连接是否活跃
    - `terminal_size: (u16, u16)` - 终端尺寸，支持动态调整窗口大小
  - `SshActor` - SSH Actor结构体（核心设计）
    - `connection: Ssh2Connection` - SSH连接实例（独占访问）
    - `message_receiver: Receiver<SshMessage>` - 消息接收器
    - `output_sender: Sender<String>` - 输出发送器，向UI发送数据
  - `Ssh2Manager` - SSH2管理器结构体
    - `actors: HashMap<String, SshActorHandle>` - Actor句柄集合，管理多个SSH Actor
    - `runtime: tokio::runtime::Runtime` - 异步运行时，处理Actor生命周期

#### 📁 UI层 (src/ui/)
- **mod.rs** - UI模块入口，导出所有UI组件和配置类型
  - `ConnectionConfig` - SSH连接配置结构体
    - `name: String` - 连接名称，用户自定义标识
    - `host: String` - 主机地址，SSH服务器IP或域名
    - `port: u16` - 端口号，默认22，支持自定义SSH端口
    - `username: String` - 用户名，SSH登录账户
    - `auth_type: AuthType` - 认证类型，支持密码和公钥认证
    - `password: Option<String>` - 密码，密码认证时使用
    - `key_file: Option<String>` - 私钥文件路径，公钥认证时使用
    - `description: String` - 连接描述，用户备注信息
  - `AuthType` - 认证类型枚举
    - `Password` - 密码认证，使用用户名密码登录
    - `PublicKey` - 公钥认证，使用SSH密钥对登录
- **connection_manager.rs** - 连接管理器，处理SSH连接配置的增删改查
  - `ConnectionManager` - 连接管理器结构体
    - `show_add_dialog: bool` - 添加对话框显示状态，控制弹窗显示
    - `edit_connection: Option<ConnectionConfig>` - 编辑中的连接，暂存用户输入
    - `selected_connection: Option<usize>` - 选中的连接索引，标识当前操作的连接
- **plugins_panel.rs** - 插件面板，展示系统监控信息
  - `PluginsPanel` - 插件面板结构体
    - `system_monitor: SystemMonitor` - 系统监控插件实例
    - `software_detector: SoftwareDetector` - 软件检测插件实例
    - `file_browser: FileBrowser` - 文件浏览器插件实例
    - `cpu_history: VecDeque<f64>` - CPU使用率历史，绘制性能曲线图
    - `memory_history: VecDeque<f64>` - 内存使用率历史，绘制性能曲线图
    - `show_system_monitor: bool` - 系统监控显示开关
    - `show_software_list: bool` - 软件列表显示开关
    - `show_file_browser: bool` - 文件浏览器显示开关
- **simple_terminal.rs** - 简化的终端面板，直接同步操作PTY
  - `SimpleTerminalPanel` - 简化终端面板结构体
    - `title: String` - 终端标题，显示在Tab标签上
    - `connection_info: String` - 连接信息，显示连接状态和服务器信息
    - `output_buffer: VecDeque<TerminalLine>` - 输出缓冲区，存储终端历史输出
    - `input_buffer: String` - 输入缓冲区，暂存用户键盘输入
    - `scroll_to_bottom: bool` - 自动滚动标志，新输出时滚动到底部
    - `is_connected: bool` - 连接状态，控制UI显示和功能可用性
    - `ssh_manager: Option<Arc<Ssh2Manager>>` - SSH2管理器引用，执行SSH操作
    - `tab_id: Option<String>` - Tab标识符，关联SSH连接
    - `current_prompt: String` - 当前提示符，显示服务器命令行提示
    - `terminal_emulator: TerminalEmulator` - 终端模拟器，处理ANSI序列
    - `has_ssh_initial_output: bool` - 初始输出标志，处理首次连接输出
- **tab_system.rs** - Tab系统核心，使用Strategy+Factory+Observer设计模式
  - `TabContent` trait - Tab内容策略接口 (Strategy Pattern)
    - `get_title()` - 获取Tab标题，显示在Tab栏
    - `get_id()` - 获取Tab唯一标识符，管理Tab生命周期
    - `show()` - 渲染Tab内容，Strategy模式的核心方法
    - `can_close()` - 是否可关闭，控制Tab关闭行为
    - `on_close()` - 关闭时清理，释放资源
    - `get_tab_type()` - 获取Tab类型，用于类型识别
  - `WelcomeTab` - 欢迎Tab实现 (Strategy Implementation)
    - `id: String` - Tab唯一标识符
    - `title: String` - Tab标题，显示为"快速连接"
  - `TerminalTab` - 终端Tab实现 (Strategy Implementation)
    - `id: String` - Tab唯一标识符
    - `title: String` - Tab标题，显示连接信息
    - `terminal: SimpleTerminalPanel` - 终端面板实例
    - `connection_config: Option<ConnectionConfig>` - 连接配置信息
  - `TabFactory` - Tab工厂 (Factory Pattern)
    - `create_welcome_tab()` - 创建欢迎Tab，工厂方法
    - `create_terminal_tab()` - 创建终端Tab，工厂方法
  - `TabEvent` - Tab事件枚举 (Observer Pattern)
    - `CreateTerminal(ConnectionConfig)` - 创建终端事件
    - `CloseTab(String)` - 关闭Tab事件
    - `SwitchTab(String)` - 切换Tab事件
    - `RenameTab(String, String)` - 重命名Tab事件
  - `TabObserver` trait - Tab观察者接口 (Observer Pattern)
    - `on_tab_event()` - 处理Tab事件，观察者模式核心方法
  - `TabManager` - Tab管理器，协调所有设计模式
    - `tabs: HashMap<String, Box<dyn TabContent>>` - Tab集合，存储所有活跃Tab
    - `active_tab_id: Option<String>` - 当前活跃Tab标识符
    - `observers: Vec<Box<dyn TabObserver>>` - 观察者列表，事件通知对象
    - `context: TabContext` - Tab上下文，共享资源和状态
    - `ssh_manager: Arc<Ssh2Manager>` - SSH2管理器，处理所有SSH连接
  - `TabContext` - Tab上下文结构体
    - `config: AppConfig` - 应用配置，全局设置
    - `connection_manager: ConnectionManager` - 连接管理器，处理连接配置
    - `plugins_panel: PluginsPanel` - 插件面板，系统监控和工具
    - `pending_connection: Option<ConnectionConfig>` - 待处理连接，Tab间通信
- **terminal_emulator.rs** - 旧版终端模拟器（已废弃，保留兼容性）
- **terminal/** - 模块化终端模拟器目录，简约优雅的实现
  - **mod.rs** - 终端模块入口，导出公共接口
  - **types.rs** - 终端相关类型定义
    - `TerminalSegment` - 终端片段结构体
      - `text: String` - 文本内容，实际显示的字符
      - `color: Option<egui::Color32>` - 前景色，文字颜色
      - `background_color: Option<egui::Color32>` - 背景色，文字背景
      - `bold: bool` - 粗体标志，文字样式
      - `italic: bool` - 斜体标志，文字样式
      - `underline: bool` - 下划线标志，文字样式
      - `inverse: bool` - 反色标志，前景背景色互换
    - `TerminalLine` - 终端行结构体
      - `segments: Vec<TerminalSegment>` - 片段列表，一行内不同样式的文本片段
    - `TerminalProcessResult` - 终端处理结果结构体
      - `lines: Vec<TerminalLine>` - 处理后的行列表，格式化的终端输出
      - `prompt_update: Option<String>` - 提示符更新，检测到的新命令提示符
  - **emulator.rs** - 核心终端模拟器，简化到<100行
    - `TerminalEmulator` - 简化终端模拟器结构体
      - `parser: vt100::Parser` - VT100解析器，处理ANSI转义序列
      - `vt100_handler: Vt100Handler` - VT100序列处理器
      - `content_extractor: ContentExtractor` - 内容提取器
      - `width: u16` - 终端宽度，字符列数
      - `height: u16` - 终端高度，字符行数
  - **vt100_handler.rs** - VT100序列处理器，专门处理ANSI转义
    - `Vt100Handler` - VT100处理器结构体
      - `handle_clear_screen()` - 处理清屏序列
      - `handle_clear_line()` - 处理清行序列
      - `handle_cursor_move()` - 处理光标移动序列
      - `handle_control_chars()` - 处理控制字符
  - **content_extractor.rs** - 内容提取器，从VT100解析结果提取显示内容
    - `ContentExtractor` - 内容提取器结构体
      - `extract_content()` - 主要提取方法，<30行
      - `extract_lines()` - 提取屏幕行内容，<30行
      - `detect_prompt()` - 检测命令提示符，<30行

#### 📁 工具层 (src/utils/)
- **mod.rs** - 工具模块入口
- **logger.rs** - 全局日志系统，支持文件日志和控制台日志
  - `Logger` - 日志器结构体
    - `log_file_path: Option<PathBuf>` - 日志文件路径，存储日志的文件位置
    - `console_enabled: bool` - 控制台输出开关，是否在终端显示日志
    - `file_enabled: bool` - 文件输出开关，是否写入日志文件
    - `min_level: LogLevel` - 最小日志级别，过滤日志输出
  - `LogLevel` - 日志级别枚举
    - `Error` - 错误级别，系统错误和异常
    - `Warn` - 警告级别，潜在问题提醒
    - `Info` - 信息级别，重要操作记录
    - `Debug` - 调试级别，详细执行信息
  - `LogEntry` - 日志条目结构体
    - `timestamp: DateTime<Local>` - 时间戳，记录日志产生时间
    - `level: LogLevel` - 日志级别，标识重要程度
    - `module: String` - 模块名称，标识日志来源
    - `message: String` - 日志消息，实际的日志内容

#### 🎯 入口文件
- **main.rs** - 应用程序入口，初始化GUI和启动Tab应用

#### 🏗️ 架构特点
1. **设计模式驱动** - Tab系统使用Strategy、Factory、Observer模式
2. **同步PTY操作** - 移除复杂异步通道，UI直接读写PTY
3. **内部可变性** - SSH管理器使用Arc<Mutex<>>实现线程安全
4. **模块化设计** - 每个模块职责单一，易于维护和扩展
5. **简约优雅** - 遵循"简单优雅"原则，避免过度复杂的实现

#### 🎯 当前架构特点 (已完成优化)

1. **模块化终端系统** ✅
   - 已拆分为4个专门模块：`emulator.rs`、`vt100_handler.rs`、`content_extractor.rs`、`types.rs`
   - 每个模块职责单一，符合单一职责原则

2. **设计模式应用** ✅
   - Strategy模式：`TabContent` trait
   - Factory模式：`TabFactory` 创建Tab
   - Observer模式：`TabEvent` 和 `TabObserver`

3. **原生SSH2实现** ✅
   - 基于`ssh2`库的原生SSH连接
   - 支持完整的ANSI转义序列和交互式工具
   - 使用异步Tokio实现高性能通信

### 实现之前的要求

1. 使用 cargo install cargo-edit + cargo upgrade 来升级依赖，必要时使用 cargo upgrade --incompatible
2. 安装依赖的时候 应该首选不冲突且版本最新的为主 也就是 cargo add xxxx
3. 最重要的一点：实现的时候 要简约+优雅 并且 在功能合理的前提下，代码一定要符合设计模式，例如单一职责
4. 不允许使用 #[allow(dead_code)] 的代码解决警告，应该去正确的实现逻辑
5. 解决问题的时候 应该从根源去解决，而非临时方案替代
6. 每次修复完成之后 将整个项目 从 cargo check 检查出来的警告 全部都要修复，如果涉及到 某个 不存在 或者 临时的占位符 没有写具体逻辑，要补充进去
7. 每一个icon和文字的组合的按钮 都应该是 左边icon右侧中文文字，且被按钮包围，且icon使用的是图标库的icon
8. 一定要注意：在Rust异步代码中，MutexGuard 会在整个使用它的作用域内持有锁。

## 代码优化计划

### 🎯 当前架构改进 (进行中)

#### 1. SSH连接架构重构 - Actor模式实现
**目标**：解决当前锁竞争问题，提升SSH连接性能和稳定性

**当前问题**：
- 多线程共享SSH连接对象导致锁竞争激烈
- 写入线程经常因锁竞争而命令发送超时
- 架构复杂，难以调试和维护

**重构方案**：采用Actor模式 + 消息传递
```rust
// 重构后的架构设计
struct SshActor {
    connection: Ssh2Connection,  // Actor独占SSH连接
    message_rx: Receiver<SshMessage>,
    output_tx: Sender<String>,
}

enum SshMessage {
    SendCommand(String),
    ReadOutput,
    Disconnect,
}
```

**优势**：
- ✅ **彻底消除锁竞争** - 单线程操作SSH连接
- ✅ **简化架构** - 清晰的消息驱动模型
- ✅ **提升性能** - 无锁设计，减少线程同步开销
- ✅ **易于扩展** - 新功能只需添加新消息类型
- ✅ **符合Rust特性** - 利用所有权系统保证内存安全

**实施计划**：
1. **阶段1** - 重构`ssh2_client.rs`，实现Actor基础架构
2. **阶段2** - 更新`SimpleTerminalPanel`集成Actor接口
3. **阶段3** - 测试验证，性能对比，清理旧代码

#### 2. VT100解析优化 ✅ (已完成)
- 实现基于屏幕内容差异的真正增量处理
- 避免重复显示相同内容
- 优化字符网格渲染性能

#### 3. 终端交互改进 (计划中)
- 添加历史记录和自动补全功能
- 改进Tab栏视觉显示
- 实现SSH实时流式输出优化

### 🔧 技术债务处理
1. **编译警告清理** - 修复所有未使用代码警告
2. **文档同步更新** - 保持README与实际架构一致
3. **单元测试补充** - 为核心模块添加测试覆盖

### 📈 性能优化目标
- SSH命令响应时间 < 100ms
- 终端滚动流畅度 60fps
- 内存使用优化，减少不必要的数据拷贝

## 项目其他说明

- utils 下 的 logger.rs 是全局应用日志系统，日志会存储到 指定的.log 文件中
  - 重要的功能的某些进度下需要记录 例如 SSH2的 连接成功(连接信息)、连接失败(失败原因)、断开等 都需要记录
- 在菜单也称为tabs中，每一个tab都成为页面，也称为终端，这里我统称tab
  - tab有两个展示方式
    - 一个是统一的 终端列表
    - 一个是终端界面
      - 终端内容区域
      - 终端输入区域
  - tab 有两个状态
    - tab_id 区分展示方式，是在点击连接按钮后立即赋值的
    - ssh 中的连接状态 就是实际的连接状态
    - tab 大致的成员
      - title
      - tab_id
      - connection_info
      - ssh2_manager
      - ssh_status
      - current_prompt SSH服务器返回的 ANSI转义序列 的完整信息中的部分信息 例如macos返回的是: `(base) ➜  ~`
      - ...
  - 终端界面细节：
    - UI
      - 字符网格方案 参考 iTerm2 的实现
      - 自然选择功能，例如 鼠标拖动可以选择段落，Ctrl+A 全选，Ctrl+C 复制
    - 底层实现
      - app 可以有多个 tab，每个tab包含一个终端，简化的同步调用，避免复杂的消息传递架构
      - SSH2 + 终端 连接、断开、接收等命令 都是需要按照 SSH2 的官方推荐的书写方案去实现 参考 <https://docs.rs/ssh2/latest/ssh2/>
      - SSH2 发送过来的消息
        0. 在UI主循环中异步读取SSH2数据，保证响应性
        1. 打印一次完整内容
        2. 将完整内容(实际内容和ANSI转义序列)交给VT100 去解析，配合各个组件实现功能
        - 要 适配 VT100 所有的 解析 功能(方法)，参考 doc/screen.rs 的实现
        - 包括 反显、清屏、光标移动、标题、图标、内容、光标位置，样式等
        - 相关文档： <https://www2.ccs.neu.edu/research/gpc/VonaUtils/vona/terminal/vtansi.htm>

~~~doc
Last login: Mon Aug 18 04:29:55 2025

[1m[7m%[27m[1m[0m                                                                               
 
]2;liguoxin@liguoxindeMacBook-Pro:~]1;~
[0m[27m[24m[J(base) [01;32m➜  [36m~[00m [K[?1h=[?2004h

iterm2 的 展示结果为：
Last login: Mon Aug 18 16:04:06 from 192.168.3.227
(base) ➜  ~
(base) ➜  ~ pwd
/Users/liguoxin
(base) ➜  ~ pwd
/Users/liguoxin
(base) ➜  ~ ls
Applications             Movies                   app
Desktop                  Music                    default.cer
Documents                Pictures                 dotTraceSnapshots
Downloads                Public                   install.sh
IdeaSnapshots            Sync                     java_error_in_idea.hprof
Library                  WeChatProjects           ui5my-rkgns
(base) ➜  ~
~~~

## 项目相关指令

~~~sh
# 初始化项目
cargo init ay-dev-tool-rust --bin

# 检查项目是否有问题
cargo check

# 运行项目
cargo run

# 格式化所有文件
cargo fmt
~~~


import { useEffect, useMemo, useRef, useState } from 'react';
import { BookOpen, Bot, Brain, Check, Copy, Database, FileText, Menu, MessageCircle, MoreHorizontal, Plus, RefreshCw, Search, Send, Settings, SlidersHorizontal, Sparkles, Square, Trash2, Upload, UserRound, X } from 'lucide-react';
import { api, isApiError, type Card, type Connection, type Lore, type Memory, type RegexScript } from './api/client';
import { useConversation } from './hooks/useConversation';
import { readPersona, readLore, saveLocal } from './lib/storage';
import { MemoryManagement } from './components/MemoryManagement';

const defaults:Connection={provider:'ollama',base_url:'http://127.0.0.1:11434',api_key:'',model:'qwen2.5-coder:3b',summary_model:'',num_gpu:15,temperature:.8,max_tokens:1024,top_p:1,frequency_penalty:0,presence_penalty:0,repetition_penalty:1};
const initials=(name:string)=>name.trim().slice(0,2).toUpperCase()||'AI';

export default function App(){
  const [cards,setCards]=useState<Card[]>([]),[cardId,setCardId]=useState('');
  const [error,setError]=useState(''),[notice,setNotice]=useState('');
  const [search,setSearch]=useState(''),[mobile,setMobile]=useState(false),[panel,setPanel]=useState<'context'|'settings'|'memory'|null>(null),[userMenuOpen,setUserMenuOpen]=useState(false),[createDialog,setCreateDialog]=useState(false);
  const [newRole,setNewRole]=useState({name:'',description:'',first_mes:'',system_prompt:''});
  const [connection,setConnection]=useState<Connection>(defaults),[saved,setSaved]=useState(false);
  const [ollamaModels,setOllamaModels]=useState<string[]>([]),[modelsLoading,setModelsLoading]=useState(false),[modelsError,setModelsError]=useState('');
  const [persona,setPersona]=useState(readPersona);
  const [lore,setLore]=useState<Lore[]>(readLore);
  const [compat,setCompat]=useState({worldbook:0,regex:0});
  const [regexScripts,setRegexScripts]=useState<RegexScript[]>([]);
    const [memories,setMemories]=useState<Memory[]>([]),[memoryTotal,setMemoryTotal]=useState(0),[stateText,setStateText]=useState('{}');
    const [workingMemory,setWorkingMemory]=useState('');
    const [authorNoteContent,setAuthorNoteContent]=useState('');
    const [authorNoteDepth,setAuthorNoteDepth]=useState(0);
    const log=useRef<HTMLDivElement>(null),fileInput=useRef<HTMLInputElement>(null),worldInput=useRef<HTMLInputElement>(null),presetInput=useRef<HTMLInputElement>(null),regexInput=useRef<HTMLInputElement>(null),gpuSaveTimer=useRef<number|null>(null),workingMemoryTimer=useRef<number|null>(null);
  const {chats,setChats,chatId,setChatId,messages,setMessages,input,setInput,busy,loading,usage,promptChars,promptTokens,abort,newChat,deleteChat,renameChat,send,regenerate}=useConversation(cardId,{persona,lore_entries:lore},regexScripts,setError,setNotice);
  const card=cards.find(x=>x.id===cardId);
  const connectionRef=useRef(connection); connectionRef.current=connection;
  const memoryRequest=useRef(0);
  const saveQueue=useRef<Promise<void>>(Promise.resolve());
  const filtered=useMemo(()=>cards.filter(x=>`${x.name} ${x.description}`.toLowerCase().includes(search.toLowerCase())),[cards,search]);

  useEffect(()=>{Promise.all([api.cards.list(),api.connection.get(),api.compatibility.get(),api.authorNote.get()]).then(([list,cfg,assets,an])=>{setCards(Array.isArray(list)?list:[]);if(Array.isArray(list)&&list[0])setCardId(list[0].id);if(!isApiError(cfg)&&cfg.connection)setConnection({...defaults,...cfg.connection});if(!isApiError(assets)){setRegexScripts(assets.regex_scripts||[]);setCompat({worldbook:assets.worldbook?.entries?.length||0,regex:assets.regex_scripts?.length||0})}if(!isApiError(an)){setAuthorNoteContent(an.content||'');setAuthorNoteDepth(an.depth||0)}}).catch(()=>setError('无法连接后端，请先运行 npm run start。'))},[]);
  useEffect(()=>{log.current?.scrollTo({top:log.current.scrollHeight,behavior:'smooth'})},[messages]);
  useEffect(()=>saveLocal('zhuo.persona',persona),[persona]);
  useEffect(()=>saveLocal('zhuo.lore',lore),[lore]);
  useEffect(()=>{if(panel==='settings'&&connection.provider==='ollama')void loadOllamaModels()},[panel,connection.provider]);
  useEffect(()=>()=>{if(gpuSaveTimer.current!==null)window.clearTimeout(gpuSaveTimer.current)},[]);
  useEffect(()=>{if(!userMenuOpen)return;const closeMenu=(event:PointerEvent)=>{if(!(event.target instanceof Element)||!event.target.closest('.user-menu-anchor'))setUserMenuOpen(false)};const handleKey=(event:KeyboardEvent)=>{if(event.key==='Escape')setUserMenuOpen(false)};document.addEventListener('pointerdown',closeMenu);document.addEventListener('keydown',handleKey);return()=>{document.removeEventListener('pointerdown',closeMenu);document.removeEventListener('keydown',handleKey)}},[userMenuOpen]);

  async function loadOllamaModels(){setModelsLoading(true);setModelsError('');try{const payload=await api.ollama.models();if(isApiError(payload))throw new Error(payload.error);setOllamaModels(Array.isArray(payload.models)?payload.models:[])}catch(e){setModelsError(`无法读取 Ollama 模型：${String(e)}`)}finally{setModelsLoading(false)}}
  async function refreshOllamaModels(){if(await saveConnection(connection,false))await loadOllamaModels()}
  async function createCard(){try{const result=await api.cards.create(newRole);if(isApiError(result))return setError(result.error);setCards(a=>[result.card,...a]);setCardId(result.card.id);setChats([]);setChatId('');setMessages([]);setCreateDialog(false);setNewRole({name:'',description:'',first_mes:'',system_prompt:''});setNotice(`已创建角色：${result.card.name}`)}catch(e){setError(`创建角色失败：${String(e)}`)}}
  async function deleteCard(item:Card){if(!confirm(`删除角色“${item.name}”？\n\n这会永久删除角色卡文件，以及该角色的全部对话、消息、长期记忆、世界设定索引和角色状态。此操作不可撤销。`))return;try{const result=await api.cards.delete(item.id);if(isApiError(result))return setError(result.error);const next=cards.filter(x=>x.id!==item.id);setCards(next);if(cardId===item.id){setCardId(next[0]?.id||'');setChats([]);setChatId('');setMessages([]);setPanel(null)}setNotice(`已删除角色：${item.name}`)}catch(e){setError(`删除角色失败：${String(e)}`)}}
  async function importCard(file?:File){try{if(!file)return;const data=await new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result).split(',')[1]||'');reader.onerror=()=>reject(reader.error);reader.readAsDataURL(file)});const x=await api.cards.import(file.name,data);if(isApiError(x))return setError(x.error);setCards(a=>a.some(v=>v.id===x.card.id)?a:[...a,x.card]);setCardId(x.card.id);setNotice(`已导入角色：${x.card.name}`)}catch(e){setError(String(e))}}
  async function importAsset(kind:'worldbook'|'preset'|'regex',file?:File){try{if(!file)return;const data=await new Promise<string>((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(String(r.result).split(',')[1]||'');r.onerror=()=>reject(r.error);r.readAsDataURL(file)});const x=await api.compatibility.import(kind,file.name,data);if(isApiError(x))return setError(x.error);if(kind==='preset'&&x.connection)setConnection({...defaults,...x.connection});if(kind==='worldbook')setCompat(v=>({...v,worldbook:x.entries||0}));if(kind==='regex'){setRegexScripts(x.scripts||[]);setCompat(v=>({...v,regex:x.scripts?.length||0}))}setNotice(kind==='worldbook'?`世界书已导入：${x.entries} 条目`:kind==='preset'?'ST 预设已应用':`Regex 已导入：${x.scripts?.length||0} 条脚本`)}catch(e){setError(String(e))}}
  async function exportCard(format:'json'|'png'){try{if(!cardId)return;const x=await api.cards.export(cardId,format);if(isApiError(x))return setError(x.error);const a=document.createElement('a');a.href=`data:${x.mime};base64,${x.data}`;a.download=x.filename;a.click();setNotice(`已导出 ${x.filename}`)}catch(e){setError(String(e))}}
  function loreHits(text:string){const hay=text.toLowerCase();return lore.filter(x=>x.enabled&&x.keys.split(',').some(k=>k.trim()&&hay.includes(k.trim().toLowerCase())))}
  async function saveConnection(next:Connection=connectionRef.current,announce=true){
    if(gpuSaveTimer.current!==null){window.clearTimeout(gpuSaveTimer.current);gpuSaveTimer.current=null}
    connectionRef.current=next;setConnection(next);setSaved(false);
    const task=saveQueue.current.then(async()=>{const result=await api.connection.save(next);if(isApiError(result))throw new Error(result.error)});
    saveQueue.current=task.catch(()=>{});
    try{await task;if(connectionRef.current===next)setSaved(true);if(announce)setNotice('连接设置已保存');return true}
    catch(e){setError(`保存连接设置失败：${String(e)}`);return false}
  }
  function changeNumGpu(value:number){const next={...connectionRef.current,num_gpu:value};connectionRef.current=next;setConnection(next);setSaved(false);if(gpuSaveTimer.current!==null)window.clearTimeout(gpuSaveTimer.current);gpuSaveTimer.current=window.setTimeout(()=>{gpuSaveTimer.current=null;void saveConnection(connectionRef.current,false)},400)}
  function loadMemory(){
    const version=++memoryRequest.current;
    if(!cardId)return;
    void Promise.all([api.memory.list(cardId),api.state.get(cardId)]).then(([m,s])=>{
      if(version!==memoryRequest.current)return;
      if(isApiError(m))throw new Error(m.error);if(isApiError(s))throw new Error(s.error);
      setMemories(m.memories||[]);setMemoryTotal(Array.isArray(m.memories)?m.memories.length:0);setStateText(JSON.stringify(s.state||{},null,2));
    }).catch(e=>{if(version===memoryRequest.current)setError(String(e))});
  }
  useEffect(()=>{setWorkingMemory('');if(chatId)loadWorkingMemory();return()=>{if(workingMemoryTimer.current!==null)clearTimeout(workingMemoryTimer.current)}},[chatId]);
  async function saveState(){try{const state=JSON.parse(stateText);if(!state||Array.isArray(state)||typeof state!=='object')throw new Error('state must be an object');const x=await api.state.save(cardId,state);if(isApiError(x))setError(x.error);else setNotice('角色状态已保存')}catch{setError('角色状态必须是合法 JSON 对象')}}
    async function saveWorkingMemory(){if(!chatId)return;try{await api.workingMemory.save(chatId,workingMemory);setNotice('工作记忆已保存')}catch(e){setError(`保存工作记忆失败：${String(e)}`)}}
        async function loadWorkingMemory(){if(!chatId)return;try{const res=await fetch(`/api/working-memory?chat=${encodeURIComponent(chatId)}`);if(res.ok){const data=await res.json();if(!data.error)setWorkingMemory(data.content||'')}}catch(e){setError(`加载工作记忆失败：${String(e)}`)}}
        async function saveAuthorNote(){try{await api.authorNote.save(authorNoteContent,authorNoteDepth);setNotice('作者注释已保存')}catch(e){setError(`保存作者注释失败：${String(e)}`)}}
        const panelButton=(kind:typeof panel,icon:React.ReactNode,label:string)=><button className={`icon-button ${panel===kind?'active':''}`} onClick={()=>setPanel(panel===kind?null:kind)} title={label}>{icon}</button>;

  return <div className="app-shell">
    <aside className={`left-sidebar ${mobile?'mobile-open':''}`}>
      <div className="brand"><div className="brand-mark"><Sparkles size={18}/></div><div><strong>Zhuo</strong><span>角色叙事工作台</span></div><button className="mobile-close" onClick={()=>setMobile(false)}><X/></button></div>
      <div className="search-box"><Search size={16}/><input value={search} onChange={e=>setSearch(e.target.value)} placeholder="搜索角色"/></div>
      <input ref={fileInput} hidden type="file" accept=".json,.png,application/json,image/png" onChange={e=>{importCard(e.target.files?.[0]);e.currentTarget.value=''}}/>
      <div className="section-label"><span>角色</span><div className="section-actions"><button title="新建角色" disabled={busy} onClick={()=>setCreateDialog(true)}><Plus size={15}/></button><button title="导入角色卡" disabled={busy} onClick={()=>fileInput.current?.click()}><Upload size={15}/></button></div></div>
      <div className="character-list">{filtered.map(x=><div key={x.id} className="character-item"><button className={`character-row ${cardId===x.id?'selected':''}`} disabled={busy} onClick={()=>{setCardId(x.id);setMobile(false)}}><span className="avatar">{initials(x.name)}</span><span className="character-copy"><strong>{x.name}</strong><small>{x.description||'未填写角色描述'}</small></span></button><button className="character-delete" title={`删除角色 ${x.name}`} aria-label={`删除角色 ${x.name}`} disabled={busy&&cardId===x.id} onClick={()=>void deleteCard(x)}><Trash2 size={14}/></button></div>)}{!filtered.length&&<div className="empty-small">{cards.length?'没有匹配的角色':'暂无角色，点击上方 + 导入角色卡'}</div>}</div>
      <div className="section-label"><span>对话</span><button disabled={busy||loading||!cardId} onClick={newChat}><Plus size={15}/></button></div>
      <div className="chat-list">{chats.map(x=><div key={x.id} className={`chat-row ${chatId===x.id?'selected':''}`}><button disabled={busy} onDoubleClick={()=>void renameChat()} onClick={()=>{setChatId(x.id);setMobile(false)}}><MessageCircle size={15}/><span>{x.title}</span></button><button className="row-delete" disabled={busy} onClick={()=>deleteChat(x.id)}><Trash2 size={14}/></button></div>)}{!chats.length&&<button className="empty-cta" disabled={busy||loading||!cardId} onClick={newChat}><Plus size={16}/>开始第一段对话</button>}</div>
      <div className="user-menu-anchor"><button className="persona-card" aria-expanded={userMenuOpen} aria-haspopup="menu" onClick={()=>setUserMenuOpen(open=>!open)}><span className="avatar user">{initials(persona.name)}</span><span><strong>{persona.name}</strong><small>个人设置与偏好</small></span><MoreHorizontal size={18}/></button>{userMenuOpen&&<div className="user-menu-popover" role="menu"><button role="menuitem" onClick={()=>{setPanel('settings');setUserMenuOpen(false)}}><Settings size={16}/>设置</button><button role="menuitem" onClick={()=>{setPanel('context');setUserMenuOpen(false)}}><UserRound size={16}/>个性与上下文</button><button role="menuitem" onClick={()=>{setPanel('memory');setUserMenuOpen(false)}}><Brain size={16}/>记忆与状态</button></div>}</div>
    </aside>
    <main className="main-stage">
      <header className="topbar"><button className="icon-button mobile-menu" onClick={()=>setMobile(true)}><Menu/></button><div className="active-character"><span className="avatar">{initials(card?.name||'')}</span><div><strong>{card?.name||'选择角色'}</strong><span><i/> {connection.model||'未配置模型'}</span></div></div><div className="top-actions">{panelButton('memory',<Brain size={18}/>,'记忆')}{panelButton('context',<BookOpen size={18}/>,'上下文')}</div></header>
      {(notice||error)&&<div className={`status ${error?'error':''}`}>{error||notice}<button onClick={()=>{setError('');setNotice('')}}><X size={14}/></button></div>}
      <div className="conversation" ref={log}>{!messages.length&&<div className="empty-chat"><div className="empty-orbit"><Bot size={34}/></div><h1>和 {card?.name||'角色'} 开始故事</h1><p>开场白、Persona 与世界信息会自动进入上下文。</p><button disabled={busy||loading||!cardId} onClick={newChat}><Plus size={17}/>新建对话</button></div>}{messages.map((m,i)=><article key={i} className={`message ${m.role}`}><span className={`avatar ${m.role==='user'?'user':''}`}>{m.role==='user'?initials(persona.name):initials(card?.name||'')}</span><div className="message-body"><div className="message-meta"><strong>{m.role==='user'?persona.name:card?.name}</strong><span>{m.role==='user'?'Persona':'角色'}</span></div><div className="message-content">{m.content||(busy&&i===messages.length-1?<span className="typing"><i/><i/><i/></span>:'')}</div>{m.content&&<div className="message-actions"><button onClick={()=>navigator.clipboard.writeText(m.content)}><Copy size={14}/>复制</button>{m.role==='assistant'&&i===messages.length-1&&<button disabled={busy||loading} onClick={regenerate}><RefreshCw size={14}/>重生成</button>}</div>}</div></article>)}</div>
      <div className="composer-wrap">{(promptChars!==null||usage)&&<div className="usage">{promptChars!==null&&<>提示词 {promptChars}/1500 字{promptTokens!==null?` · 估算 ${promptTokens} tokens`:''}</>}{promptChars!==null&&usage?' · ':''}{usage&&`本轮 ${usage} tokens`}</div>}<div className="composer"><textarea rows={1} value={input} disabled={busy||loading||!cardId} onChange={e=>setInput(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.nativeEvent.isComposing){e.preventDefault();send()}}} placeholder={chatId?`以 ${persona.name} 的身份发送消息…`:'新建对话并发送消息…'}/><button className={`send ${busy?'stop':''}`} onClick={()=>busy?abort.current?.abort():send()} disabled={!busy&&(!input.trim()||loading||!cardId)}>{busy?<Square size={17}/>:<Send size={18}/>}</button></div><div className="composer-hint"><span>Enter 发送 · Shift + Enter 换行</span><span>{loreHits(input).length?`${loreHits(input).length} 条 World Info 关键词匹配`:'上下文自动管理'}</span></div></div>
    </main>
    {panel&&<aside className="right-panel"><div className="panel-head"><div><strong>{panel==='settings'?'连接与生成':panel==='memory'?'记忆与状态':'上下文工作台'}</strong><span>{card?.name}</span></div><button onClick={()=>setPanel(null)}><X size={18}/></button></div>
      {panel==='settings'&&<div className="panel-scroll form-stack">
        <Title icon={<Database/>}>连接设置</Title>
        <label>服务类型<select value={connection.provider} onChange={e=>setConnection({...connection,provider:e.target.value})}><option value="ollama">Ollama</option><option value="openai">OpenAI Compatible</option></select></label>
        <Field label="Base URL" value={connection.base_url} set={v=>setConnection({...connection,base_url:v})}/>
        <Field label="API Key" value={connection.api_key} set={v=>setConnection({...connection,api_key:v})} password/>
        <button className="primary wide" onClick={refreshOllamaModels} disabled={modelsLoading||connection.provider!=='ollama'}><RefreshCw size={16}/>{modelsLoading?'正在读取模型…':'刷新本地模型列表'}</button>
        {modelsError&&<div className="status error">{modelsError}</div>}
        {connection.provider==='openai'?<><Field label="主聊天模型" value={connection.model} set={v=>setConnection({...connection,model:v})}/><Field label="后台记忆提取模型（留空关闭）" value={connection.summary_model} set={v=>setConnection({...connection,summary_model:v})}/></>:<>
        <label>主聊天模型<select value={connection.model} onChange={e=>void saveConnection({...connection,model:e.target.value},false)}>{!ollamaModels.includes(connection.model)&&<option value={connection.model}>{connection.model}</option>}{ollamaModels.map(model=><option key={model} value={model}>{model}</option>)}</select></label>
        <label>后台记忆提取模型<select value={connection.summary_model} onChange={e=>void saveConnection({...connection,summary_model:e.target.value},false)}><option value="">关闭自动记忆提取</option>{!ollamaModels.includes(connection.summary_model)&&connection.summary_model&&<option value={connection.summary_model}>{connection.summary_model}</option>}{ollamaModels.map(model=><option key={model} value={model}>{model}</option>)}</select></label></>}
        <label>显存分配层数（num_gpu）<b>{connection.num_gpu}</b><input type="range" min="0" max="99" step="1" value={connection.num_gpu} onChange={e=>changeNumGpu(Number(e.target.value))}/></label>
        <small className="settings-help">6GB 显卡建议 15 层；12GB 以上可尝试 99 层。后台提取模型固定使用 CPU。</small>
        <Title icon={<SlidersHorizontal/>}>生成参数</Title>
        <label>Temperature <b>{connection.temperature.toFixed(1)}</b><input type="range" min="0" max="2" step="0.1" value={connection.temperature} onChange={e=>setConnection({...connection,temperature:Number(e.target.value)})}/></label>
        <label>Top P<input type="number" min="0" max="1" step="0.05" value={connection.top_p} onChange={e=>setConnection({...connection,top_p:Number(e.target.value)})}/></label>
        <label>Frequency Penalty<input type="number" min="-2" max="2" step="0.1" value={connection.frequency_penalty} onChange={e=>setConnection({...connection,frequency_penalty:Number(e.target.value)})}/></label>
        <label>Presence Penalty<input type="number" min="-2" max="2" step="0.1" value={connection.presence_penalty} onChange={e=>setConnection({...connection,presence_penalty:Number(e.target.value)})}/></label>
        <label>Repetition Penalty<input type="number" min="0" max="2" step="0.05" value={connection.repetition_penalty} onChange={e=>setConnection({...connection,repetition_penalty:Number(e.target.value)})}/></label>
        <label>最大输出 Tokens<input type="number" value={connection.max_tokens} onChange={e=>setConnection({...connection,max_tokens:Number(e.target.value)})}/></label>
        <button className="primary wide" onClick={()=>void saveConnection()}>{saved?<Check size={16}/>:<Settings size={16}/>}保存设置</button>
                <Title icon={<FileText/>}>作者注释</Title>
                <label>内容<textarea rows={4} value={authorNoteContent} onChange={e=>setAuthorNoteContent(e.target.value)} placeholder="保持角色扮演风格..."/></label>
                <label>注入深度 <b>{authorNoteDepth}</b><input type="range" min="0" max="5" step="1" value={authorNoteDepth} onChange={e=>setAuthorNoteDepth(Number(e.target.value))}/><small>0=最新消息后，5=最远</small></label>
                <button className="secondary wide" onClick={saveAuthorNote}>保存作者注释</button>
                <Title icon={<Sparkles/>}>SillyTavern 资产</Title>
        <input ref={worldInput} hidden type="file" accept=".json" onChange={e=>{importAsset('worldbook',e.target.files?.[0]);e.currentTarget.value=''}}/>
        <input ref={presetInput} hidden type="file" accept=".json" onChange={e=>{importAsset('preset',e.target.files?.[0]);e.currentTarget.value=''}}/>
        <input ref={regexInput} hidden type="file" accept=".json" onChange={e=>{importAsset('regex',e.target.files?.[0]);e.currentTarget.value=''}}/>
        <div className="compat-grid"><button onClick={()=>worldInput.current?.click()}><BookOpen/>导入世界书<small>{compat.worldbook} 条</small></button><button onClick={()=>presetInput.current?.click()}><SlidersHorizontal/>导入预设</button><button onClick={()=>regexInput.current?.click()}><RefreshCw/>导入 Regex<small>{compat.regex} 条</small></button><button onClick={()=>exportCard('json')}><Database/>导出 V2 JSON</button><button onClick={()=>exportCard('png')}><Sparkles/>导出 V2 PNG</button></div>
        <MemoryManagement cards={cards} cardId={cardId} />
      </div>}
      {panel==='context'&&<div className="panel-scroll form-stack"><Title icon={<UserRound/>}>当前 Persona</Title><Field label="名称" value={persona.name} set={v=>setPersona({...persona,name:v})}/><label>描述<textarea rows={5} value={persona.description} onChange={e=>setPersona({...persona,description:e.target.value})} placeholder="身份、外观、说话方式和背景…"/></label><div className="panel-section-title split"><span><BookOpen/>World Info</span><button onClick={()=>setLore(a=>[...a,{id:crypto.randomUUID(),keys:'',content:'',enabled:true}])}><Plus size={15}/>条目</button></div>{lore.map(x=><div className="lore-card" key={x.id}><div className="lore-top"><button className={`switch ${x.enabled?'on':''}`} onClick={()=>setLore(a=>a.map(v=>v.id===x.id?{...v,enabled:!v.enabled}:v))}><i/></button><span>{x.enabled?'已启用':'已停用'}</span><button className="danger-icon" onClick={()=>setLore(a=>a.filter(v=>v.id!==x.id))}><Trash2 size={14}/></button></div><input value={x.keys} onChange={e=>setLore(a=>a.map(v=>v.id===x.id?{...v,keys:e.target.value}:v))} placeholder="触发词，用逗号分隔"/><textarea rows={4} value={x.content} onChange={e=>setLore(a=>a.map(v=>v.id===x.id?{...v,content:e.target.value}:v))} placeholder="命中后注入的世界设定…"/></div>)}{!lore.length&&<Empty icon={<BookOpen/>} text="暂无 World Info 条目"/>}</div>}
      {panel==='memory'&&<div className="panel-scroll form-stack"><div className="metric-row"><div><span>长期记忆</span><strong>{memoryTotal}</strong></div><div><span>当前消息</span><strong>{messages.length}</strong></div></div><div className="panel-section-title split"><span><Brain/>记忆库</span><button onClick={loadMemory}><RefreshCw size={14}/>刷新</button></div><div className="memory-list">{memories.map(x=><div key={x.id}><p>{x.summary}</p><small>{x.source_round?`自动提取 #${x.source_round}`:'手动添加'} · {x.created_at}</small></div>)}{!memories.length&&<Empty icon={<Brain/>} text="还没有长期记忆"/>}</div><Title icon={<FileText/>}>工作记忆</Title><textarea className="code-editor" rows={8} placeholder="当前会话的临时笔记，切换会话不共享，不计入长期记忆" onChange={e=>{setWorkingMemory(e.target.value);if(workingMemoryTimer.current!==null)clearTimeout(workingMemoryTimer.current);workingMemoryTimer.current=setTimeout(()=>saveWorkingMemory(),500)}} onBlur={saveWorkingMemory} value={workingMemory}/><small className="settings-help">失焦或停止输入 500ms 后自动保存。切换会话时自动加载对应记忆。</small><Title icon={<Database/>}>角色状态 JSON</Title><textarea className="code-editor" rows={10} value={stateText} onChange={e=>setStateText(e.target.value)}/><button className="secondary wide" onClick={saveState}>保存角色状态</button></div>}
    </aside>}
    {createDialog&&<div className="create-modal-backdrop"><form className="create-modal form-stack" onSubmit={e=>{e.preventDefault();void createCard()}}><div className="create-modal-head"><div><strong>新建角色</strong><span>创建一个本地角色卡</span></div><button type="button" onClick={()=>setCreateDialog(false)} aria-label="关闭"><X size={18}/></button></div><label>角色名称<input autoFocus required maxLength={100} value={newRole.name} onChange={e=>setNewRole({...newRole,name:e.target.value})} placeholder="例如：林间旅人"/></label><label>角色背景<textarea rows={3} value={newRole.description} onChange={e=>setNewRole({...newRole,description:e.target.value})} placeholder="简单介绍角色的身份与背景"/></label><label>开场白<textarea rows={3} value={newRole.first_mes} onChange={e=>setNewRole({...newRole,first_mes:e.target.value})} placeholder="角色第一次对用户说的话"/></label><label>角色指令<textarea rows={4} value={newRole.system_prompt} onChange={e=>setNewRole({...newRole,system_prompt:e.target.value})} placeholder="角色的性格、行为和回复要求"/></label><div className="create-modal-actions"><button type="button" className="secondary" onClick={()=>setCreateDialog(false)}>取消</button><button type="submit" className="primary" disabled={!newRole.name.trim()}>创建角色</button></div></form></div>}
    {mobile&&<button className="backdrop" onClick={()=>setMobile(false)} aria-label="关闭导航"/>}
  </div>
}

function Title({icon,children}:{icon:React.ReactNode;children:React.ReactNode}){return <div className="panel-section-title">{icon}{children}</div>}
function Field({label,value,set,password=false}:{label:string;value:string;set:(v:string)=>void;password?:boolean}){return <label>{label}<input type={password?'password':'text'} value={value} onChange={e=>set(e.target.value)}/></label>}
function Empty({icon,text}:{icon:React.ReactNode;text:string}){return <div className="panel-empty">{icon}<p>{text}</p></div>}

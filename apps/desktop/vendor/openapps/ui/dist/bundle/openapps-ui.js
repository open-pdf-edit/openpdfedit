import{a as l,b as U,c as ie,d as ae}from"./chunk-LCQWCHVU.js";var y=class extends Error{code;status;balance;detail;constructor(e,t,r=0,s,i){super(t),this.name="OpenAppsError",this.code=e,this.status=r,this.balance=s,this.detail=i}get isAuthError(){return this.code==="unauthorized"}},bt={400:"bad_request",401:"unauthorized",402:"insufficient_balance",404:"not_found",409:"conflict",429:"rate_limited"};function Ge(n,e){let t=e&&typeof e=="object"?e.error:void 0,r=t&&typeof t=="object"?t:void 0,s=r?.code??bt[n]??"internal",i=r?.message??`request failed with status ${n}`,a;if(s==="insufficient_balance"){let u=/-?\d+/.exec(i);u&&(a=Number(u[0]))}return new y(s,i,n,a,r)}function ge(n=null){let e=n;return{get:()=>e,set:t=>{e=t}}}function vt(n="openapps.session"){let e=null;try{e=typeof localStorage<"u"?localStorage:null,e?.setItem(n,e.getItem(n)??""),e?.getItem(n)===""&&e.removeItem(n)}catch{e=null}if(!e)return ge();let t=e;return{get(){let r=t.getItem(n);if(!r)return null;try{let s=JSON.parse(r);return s.accessToken&&s.refreshToken?s:null}catch{return null}},set(r){r?t.setItem(n,JSON.stringify(r)):t.removeItem(n)}}}function We(){try{return typeof localStorage<"u"?vt():ge()}catch{return ge()}}var yt=new Set(["confirmed","failed","expired"]),V=class{baseUrl;#r;#n;#o;#l;#s=null;#i=null;constructor(e){this.baseUrl=e.baseUrl.replace(/\/+$/,""),this.#r=e.appKey,this.#n=e.store??We();let t=e.fetch??globalThis.fetch;if(!t)throw new y("network","no fetch implementation available; pass one via options.fetch");this.#o=(r,s)=>t(r,s),this.#l=e.onAuthChange}get session(){return this.#n.get()}get isLoggedIn(){return this.#n.get()!==null}#t(e){this.#n.set(e),this.#l?.(e)}adoptSession(e,t){this.#t({accessToken:e,refreshToken:t})}clearSession(){this.#t(null)}async#e(e,t={}){let r=t.auth??"none";if(r!=="none"&&!this.#n.get())throw new y("unauthorized","not logged in");if(r==="app+bearer"&&!this.#r)throw new y("unauthorized","this call needs an app key; construct OpenApps with { appKey }");return this.#a(e,t,r,!0)}async#a(e,t,r,s){let i=`${this.baseUrl}${e}`;if(t.query){let h=new URLSearchParams;for(let[f,C]of Object.entries(t.query))C!==void 0&&h.set(f,String(C));let b=h.toString();b&&(i+=`?${b}`)}let a={accept:"application/json"};t.body!==void 0&&(a["content-type"]="application/json"),r!=="none"&&(a.authorization=`Bearer ${this.#n.get()?.accessToken??""}`),r==="app+bearer"&&this.#r&&(a["x-openapps-app-key"]=this.#r);let u;try{u=await this.#o(i,{method:t.method??"GET",headers:a,body:t.body===void 0?void 0:JSON.stringify(t.body),signal:t.signal})}catch(h){throw h instanceof Error&&h.name==="AbortError"?h:new y("network",h instanceof Error?h.message:"network request failed")}if(u.status===401&&r!=="none"&&s&&await this.#d())return this.#a(e,t,r,!1);let d=await this.#c(u);if(!u.ok){let h=Ge(u.status,d);throw h.code==="unauthorized"&&r!=="none"&&this.#t(null),h}return d}async#c(e){if(e.status===204)return null;let t=await e.text();if(!t)return null;try{return JSON.parse(t)}catch{throw new y(e.ok?"internal":"network",`expected JSON, got: ${t.slice(0,200)}`,e.status)}}#d(){if(this.#s)return this.#s;let e=this.#n.get();return e?(this.#s=(async()=>{try{let t=await this.#a("/v1/auth/refresh",{method:"POST",body:{refresh_token:e.refreshToken}},"none",!1),r={accessToken:t.access_token,refreshToken:t.refresh_token};return this.#t(r),r}catch{return this.#t(null),null}finally{this.#s=null}})(),this.#s):Promise.resolve(null)}auth={methods:async e=>(await this.#e("/v1/auth/methods",{signal:e})).methods,challenge:(e,t,r)=>this.#e("/v1/auth/challenge",{method:"POST",body:{namespace:e,address:t},signal:r}),verify:async(e,t,r={})=>{let s=await this.#e("/v1/auth/verify",{method:"POST",body:{challenge_id:e,proof:t,referral_code:r.referralCode},signal:r.signal});return this.#t({accessToken:s.access_token,refreshToken:s.refresh_token}),s},googleStartUrl:(e,t)=>{let r=new URLSearchParams;e&&r.set("return_to",e),t&&r.set("ref",t);let s=r.toString();return`${this.baseUrl}/v1/auth/oidc/google/start${s?`?${s}`:""}`},completeRedirect:(e={})=>{let t=kt(e,"code");return t?this.#i?this.#i:(this.#i=(async()=>{try{let r=await this.#e("/v1/auth/oidc/exchange",{method:"POST",body:{code:t},signal:e.signal});return this.#t({accessToken:r.access_token,refreshToken:r.refresh_token}),e.hash===void 0&&e.url===void 0&&typeof history<"u"&&typeof location<"u"&&history.replaceState({},"",location.pathname+location.search),r}finally{this.#i=null}})(),this.#i):Promise.resolve(null)},me:e=>this.#e("/v1/me",{auth:"bearer",signal:e}),logout:async e=>{try{await this.#e("/v1/auth/logout",{method:"POST",auth:"bearer",signal:e})}finally{this.#t(null)}},linkChallenge:(e,t,r)=>this.#e("/v1/auth/link/challenge",{method:"POST",auth:"bearer",body:{namespace:e,address:t},signal:r}),linkVerify:(e,t,r={})=>this.#e("/v1/auth/link/verify",{method:"POST",auth:"bearer",body:{challenge_id:e,proof:t,merge:r.merge??!1},signal:r.signal}),googleLinkStart:async(e,t={})=>(await this.#e("/v1/auth/link/oidc/google/start",{method:"POST",auth:"bearer",body:{return_to:e,merge:t.merge??!1},signal:t.signal})).auth_url,completeLinkRedirect:(e={})=>{let t=je(e),r=t.get("linked"),s=t.get("link_conflict"),i=t.get("link_blocked"),a=t.get("link_error");if(!r&&!s&&!i&&!a)return null;if(e.hash===void 0&&e.url===void 0&&typeof history<"u"&&history.replaceState({},"",location.pathname+location.search),a)return{status:"error",message:a};if(i){let u=(t.get("clashes")??"").split(",").filter(Boolean),d=u.map(h=>({google:"Google",eip155:"wallet",nostr:"Nostr"})[h]??h).join(" and ");return{status:"blocked",namespaces:u,message:`That Google account belongs to another account which also has a ${d} sign-in, and so does this one. Disconnect it from the other account first.`}}return s?{status:"conflict",namespace:s,balance:Number(t.get("balance")??0)}:{status:"linked",namespace:r,merged:t.get("merged")==="1",credits:Number(t.get("credits")??0)}},unlink:(e,t)=>this.#e(`/v1/auth/link/${encodeURIComponent(e)}`,{method:"DELETE",auth:"bearer",signal:t})};credits={balance:async e=>(await this.#e("/v1/credits/balance",{auth:"bearer",signal:e})).balance,deduct:(e,t,r,s)=>this.#e("/v1/credits/deduct",{method:"POST",auth:"app+bearer",body:{amount:e,reason:t,idempotency_key:r},signal:s}),history:(e={})=>this.#e("/v1/credits/history",{auth:"bearer",query:{cursor:e.cursor,limit:e.limit},signal:e.signal})};payments={packages:e=>this.#e("/v1/payments/packages",{signal:e}),stripeCheckout:(e,t={})=>this.#e("/v1/payments/stripe/checkout",{method:"POST",auth:"bearer",body:{package_id:e,return_to:t.returnTo===null?void 0:t.returnTo??wt()},signal:t.signal}),ethDepositAddress:(e,t)=>this.#e("/v1/payments/eth/deposit-address",{method:"POST",auth:"bearer",body:{package_id:e},signal:t}),lightningInvoice:(e,t)=>this.#e("/v1/payments/lightning/invoice",{method:"POST",auth:"bearer",body:{package_id:e},signal:t}),list:e=>this.#e("/v1/payments/topups",{auth:"bearer",signal:e}),get:(e,t)=>this.#e(`/v1/payments/topups/${encodeURIComponent(e)}`,{auth:"bearer",signal:t}),waitFor:async(e,t={})=>{let r=t.intervalMs??2e3,s=Date.now()+(t.timeoutMs??900*1e3);for(;;){t.signal?.throwIfAborted();try{let i=await this.payments.get(e,t.signal);if(t.onPoll?.(i),yt.has(i.status))return i}catch(i){if(i instanceof y&&i.code!=="network"||!(i instanceof y))throw i}if(Date.now()+r>s)throw new y("timeout",`top-up ${e} was still pending after the timeout`);await $t(r,t.signal)}}};referral={code:(e,t)=>this.#e("/v1/referral/code",{auth:"bearer",query:{app:e},signal:t}),apply:(e,t)=>this.#e("/v1/referral/apply",{method:"POST",auth:"bearer",body:{code:e},signal:t}),earnings:e=>this.#e("/v1/referral/earnings",{auth:"bearer",signal:e}),referees:e=>this.#e("/v1/referral/referees",{auth:"bearer",signal:e})}};function wt(){if(!(typeof location>"u"))return`${location.origin}${location.pathname}${location.search}`}function je(n){if(n.url!==void 0){let t=n.url,r=t.indexOf("#"),s=t.indexOf("?"),i=r>=0?t.slice(r+1):"",u=s>=0&&(r<0||s<r)?t.slice(s+1,r>=0?r:void 0):"",d=new URLSearchParams(i),h=new URLSearchParams(u);return{get:b=>d.get(b)??h.get(b)}}let e=n.hash??(typeof location>"u"?"":location.hash);return new URLSearchParams(e.replace(/^#/,""))}function kt(n,e){return je(n).get(e)}function $t(n,e){return new Promise((t,r)=>{let s=setTimeout(()=>{e?.removeEventListener("abort",i),t()},n),i=()=>{clearTimeout(s),r(e?.reason??new Error("aborted"))};e?.addEventListener("abort",i,{once:!0})})}var be="openapps.referral";function xt(){try{let n=localStorage.getItem(be);if(!n)return null;let e=JSON.parse(n);return typeof e?.code!="string"||typeof e?.at!="number"?null:{code:e.code,at:e.at}}catch{return null}}function oe(){if(!(typeof location>"u"))try{return new URLSearchParams(location.search).get("ref")??void 0}catch{return}}function ve(){let n=oe();if(n)try{localStorage.setItem(be,JSON.stringify({code:n,at:Date.now()}))}catch{}}function ye(){let n=xt();if(n){if(Date.now()-n.at>2592e6){R();return}return n.code}}function R(){try{localStorage.removeItem(be)}catch{}}var K=null;function Be(n){return K=new V(n),ve(),k(),K}function _t(){return K}function Fe(n,e){if(n)return n;if(K)return K;if(e)return Be({baseUrl:e});throw new Error("no OpenApps client: call configure({ baseUrl }) or set base-url on the element")}var we=new Set;function ke(n){return we.add(n),()=>we.delete(n)}function k(){for(let n of we)n()}var le=globalThis,ce=le.ShadowRoot&&(le.ShadyCSS===void 0||le.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,$e=Symbol(),Ve=new WeakMap,J=class{constructor(e,t,r){if(this._$cssResult$=!0,r!==$e)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=e,this.t=t}get styleSheet(){let e=this.o,t=this.t;if(ce&&e===void 0){let r=t!==void 0&&t.length===1;r&&(e=Ve.get(t)),e===void 0&&((this.o=e=new CSSStyleSheet).replaceSync(this.cssText),r&&Ve.set(t,e))}return e}toString(){return this.cssText}},Ke=n=>new J(typeof n=="string"?n:n+"",void 0,$e),$=(n,...e)=>{let t=n.length===1?n[0]:e.reduce((r,s,i)=>r+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(s)+n[i+1],n[0]);return new J(t,n,$e)},Je=(n,e)=>{if(ce)n.adoptedStyleSheets=e.map(t=>t instanceof CSSStyleSheet?t:t.styleSheet);else for(let t of e){let r=document.createElement("style"),s=le.litNonce;s!==void 0&&r.setAttribute("nonce",s),r.textContent=t.cssText,n.appendChild(r)}},xe=ce?n=>n:n=>n instanceof CSSStyleSheet?(e=>{let t="";for(let r of e.cssRules)t+=r.cssText;return Ke(t)})(n):n;var{is:St,defineProperty:Ct,getOwnPropertyDescriptor:Et,getOwnPropertyNames:Pt,getOwnPropertySymbols:Tt,getPrototypeOf:At}=Object,de=globalThis,Ye=de.trustedTypes,Nt=Ye?Ye.emptyScript:"",Rt=de.reactiveElementPolyfillSupport,Y=(n,e)=>n,Q={toAttribute(n,e){switch(e){case Boolean:n=n?Nt:null;break;case Object:case Array:n=n==null?n:JSON.stringify(n)}return n},fromAttribute(n,e){let t=n;switch(e){case Boolean:t=n!==null;break;case Number:t=n===null?null:Number(n);break;case Object:case Array:try{t=JSON.parse(n)}catch{t=null}}return t}},ue=(n,e)=>!St(n,e),Qe={attribute:!0,type:String,converter:Q,reflect:!1,useDefault:!1,hasChanged:ue};Symbol.metadata??=Symbol("metadata"),de.litPropertyMetadata??=new WeakMap;var T=class extends HTMLElement{static addInitializer(e){this._$Ei(),(this.l??=[]).push(e)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(e,t=Qe){if(t.state&&(t.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(e)&&((t=Object.create(t)).wrapped=!0),this.elementProperties.set(e,t),!t.noAccessor){let r=Symbol(),s=this.getPropertyDescriptor(e,r,t);s!==void 0&&Ct(this.prototype,e,s)}}static getPropertyDescriptor(e,t,r){let{get:s,set:i}=Et(this.prototype,e)??{get(){return this[t]},set(a){this[t]=a}};return{get:s,set(a){let u=s?.call(this);i?.call(this,a),this.requestUpdate(e,u,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(e){return this.elementProperties.get(e)??Qe}static _$Ei(){if(this.hasOwnProperty(Y("elementProperties")))return;let e=At(this);e.finalize(),e.l!==void 0&&(this.l=[...e.l]),this.elementProperties=new Map(e.elementProperties)}static finalize(){if(this.hasOwnProperty(Y("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(Y("properties"))){let t=this.properties,r=[...Pt(t),...Tt(t)];for(let s of r)this.createProperty(s,t[s])}let e=this[Symbol.metadata];if(e!==null){let t=litPropertyMetadata.get(e);if(t!==void 0)for(let[r,s]of t)this.elementProperties.set(r,s)}this._$Eh=new Map;for(let[t,r]of this.elementProperties){let s=this._$Eu(t,r);s!==void 0&&this._$Eh.set(s,t)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(e){let t=[];if(Array.isArray(e)){let r=new Set(e.flat(1/0).reverse());for(let s of r)t.unshift(xe(s))}else e!==void 0&&t.push(xe(e));return t}static _$Eu(e,t){let r=t.attribute;return r===!1?void 0:typeof r=="string"?r:typeof e=="string"?e.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(e=>this.enableUpdating=e),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(e=>e(this))}addController(e){(this._$EO??=new Set).add(e),this.renderRoot!==void 0&&this.isConnected&&e.hostConnected?.()}removeController(e){this._$EO?.delete(e)}_$E_(){let e=new Map,t=this.constructor.elementProperties;for(let r of t.keys())this.hasOwnProperty(r)&&(e.set(r,this[r]),delete this[r]);e.size>0&&(this._$Ep=e)}createRenderRoot(){let e=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Je(e,this.constructor.elementStyles),e}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(e=>e.hostConnected?.())}enableUpdating(e){}disconnectedCallback(){this._$EO?.forEach(e=>e.hostDisconnected?.())}attributeChangedCallback(e,t,r){this._$AK(e,r)}_$ET(e,t){let r=this.constructor.elementProperties.get(e),s=this.constructor._$Eu(e,r);if(s!==void 0&&r.reflect===!0){let i=(r.converter?.toAttribute!==void 0?r.converter:Q).toAttribute(t,r.type);this._$Em=e,i==null?this.removeAttribute(s):this.setAttribute(s,i),this._$Em=null}}_$AK(e,t){let r=this.constructor,s=r._$Eh.get(e);if(s!==void 0&&this._$Em!==s){let i=r.getPropertyOptions(s),a=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:Q;this._$Em=s;let u=a.fromAttribute(t,i.type);this[s]=u??this._$Ej?.get(s)??u,this._$Em=null}}requestUpdate(e,t,r,s=!1,i){if(e!==void 0){let a=this.constructor;if(s===!1&&(i=this[e]),r??=a.getPropertyOptions(e),!((r.hasChanged??ue)(i,t)||r.useDefault&&r.reflect&&i===this._$Ej?.get(e)&&!this.hasAttribute(a._$Eu(e,r))))return;this.C(e,t,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(e,t,{useDefault:r,reflect:s,wrapped:i},a){r&&!(this._$Ej??=new Map).has(e)&&(this._$Ej.set(e,a??t??this[e]),i!==!0||a!==void 0)||(this._$AL.has(e)||(this.hasUpdated||r||(t=void 0),this._$AL.set(e,t)),s===!0&&this._$Em!==e&&(this._$Eq??=new Set).add(e))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(t){Promise.reject(t)}let e=this.scheduleUpdate();return e!=null&&await e,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[s,i]of this._$Ep)this[s]=i;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[s,i]of r){let{wrapped:a}=i,u=this[s];a!==!0||this._$AL.has(s)||u===void 0||this.C(s,void 0,i,u)}}let e=!1,t=this._$AL;try{e=this.shouldUpdate(t),e?(this.willUpdate(t),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(t)):this._$EM()}catch(r){throw e=!1,this._$EM(),r}e&&this._$AE(t)}willUpdate(e){}_$AE(e){this._$EO?.forEach(t=>t.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(e)),this.updated(e)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(e){return!0}update(e){this._$Eq&&=this._$Eq.forEach(t=>this._$ET(t,this[t])),this._$EM()}updated(e){}firstUpdated(e){}};T.elementStyles=[],T.shadowRootOptions={mode:"open"},T[Y("elementProperties")]=new Map,T[Y("finalized")]=new Map,Rt?.({ReactiveElement:T}),(de.reactiveElementVersions??=[]).push("2.1.2");var Ae=globalThis,Ze=n=>n,he=Ae.trustedTypes,Xe=he?he.createPolicy("lit-html",{createHTML:n=>n}):void 0,it="$lit$",M=`lit$${Math.random().toFixed(9).slice(2)}$`,at="?"+M,Mt=`<${at}>`,z=document,X=()=>z.createComment(""),ee=n=>n===null||typeof n!="object"&&typeof n!="function",Ne=Array.isArray,Lt=n=>Ne(n)||typeof n?.[Symbol.iterator]=="function",_e=`[ 	
\f\r]`,Z=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,et=/-->/g,tt=/>/g,I=RegExp(`>|${_e}(?:([^\\s"'>=/]+)(${_e}*=${_e}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),rt=/'/g,nt=/"/g,ot=/^(?:script|style|textarea|title)$/i,Re=n=>(e,...t)=>({_$litType$:n,strings:e,values:t}),o=Re(1),pe=Re(2),yr=Re(3),q=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),st=new WeakMap,O=z.createTreeWalker(z,129);function lt(n,e){if(!Ne(n)||!n.hasOwnProperty("raw"))throw Error("invalid template strings array");return Xe!==void 0?Xe.createHTML(e):e}var Ut=(n,e)=>{let t=n.length-1,r=[],s,i=e===2?"<svg>":e===3?"<math>":"",a=Z;for(let u=0;u<t;u++){let d=n[u],h,b,f=-1,C=0;for(;C<d.length&&(a.lastIndex=C,b=a.exec(d),b!==null);)C=a.lastIndex,a===Z?b[1]==="!--"?a=et:b[1]!==void 0?a=tt:b[2]!==void 0?(ot.test(b[2])&&(s=RegExp("</"+b[2],"g")),a=I):b[3]!==void 0&&(a=I):a===I?b[0]===">"?(a=s??Z,f=-1):b[1]===void 0?f=-2:(f=a.lastIndex-b[2].length,h=b[1],a=b[3]===void 0?I:b[3]==='"'?nt:rt):a===nt||a===rt?a=I:a===et||a===tt?a=Z:(a=I,s=void 0);let N=a===I&&n[u+1].startsWith("/>")?" ":"";i+=a===Z?d+Mt:f>=0?(r.push(h),d.slice(0,f)+it+d.slice(f)+M+N):d+M+(f===-2?u:N)}return[lt(n,i+(n[t]||"<?>")+(e===2?"</svg>":e===3?"</math>":"")),r]},te=class n{constructor({strings:e,_$litType$:t},r){let s;this.parts=[];let i=0,a=0,u=e.length-1,d=this.parts,[h,b]=Ut(e,t);if(this.el=n.createElement(h,r),O.currentNode=this.el.content,t===2||t===3){let f=this.el.content.firstChild;f.replaceWith(...f.childNodes)}for(;(s=O.nextNode())!==null&&d.length<u;){if(s.nodeType===1){if(s.hasAttributes())for(let f of s.getAttributeNames())if(f.endsWith(it)){let C=b[a++],N=s.getAttribute(f).split(M),se=/([.?@])?(.*)/.exec(C);d.push({type:1,index:i,name:se[2],strings:N,ctor:se[1]==="."?Ce:se[1]==="?"?Ee:se[1]==="@"?Pe:G}),s.removeAttribute(f)}else f.startsWith(M)&&(d.push({type:6,index:i}),s.removeAttribute(f));if(ot.test(s.tagName)){let f=s.textContent.split(M),C=f.length-1;if(C>0){s.textContent=he?he.emptyScript:"";for(let N=0;N<C;N++)s.append(f[N],X()),O.nextNode(),d.push({type:2,index:++i});s.append(f[C],X())}}}else if(s.nodeType===8)if(s.data===at)d.push({type:2,index:i});else{let f=-1;for(;(f=s.data.indexOf(M,f+1))!==-1;)d.push({type:7,index:i}),f+=M.length-1}i++}}static createElement(e,t){let r=z.createElement("template");return r.innerHTML=e,r}};function H(n,e,t=n,r){if(e===q)return e;let s=r!==void 0?t._$Co?.[r]:t._$Cl,i=ee(e)?void 0:e._$litDirective$;return s?.constructor!==i&&(s?._$AO?.(!1),i===void 0?s=void 0:(s=new i(n),s._$AT(n,t,r)),r!==void 0?(t._$Co??=[])[r]=s:t._$Cl=s),s!==void 0&&(e=H(n,s._$AS(n,e.values),s,r)),e}var Se=class{constructor(e,t){this._$AV=[],this._$AN=void 0,this._$AD=e,this._$AM=t}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(e){let{el:{content:t},parts:r}=this._$AD,s=(e?.creationScope??z).importNode(t,!0);O.currentNode=s;let i=O.nextNode(),a=0,u=0,d=r[0];for(;d!==void 0;){if(a===d.index){let h;d.type===2?h=new re(i,i.nextSibling,this,e):d.type===1?h=new d.ctor(i,d.name,d.strings,this,e):d.type===6&&(h=new Te(i,this,e)),this._$AV.push(h),d=r[++u]}a!==d?.index&&(i=O.nextNode(),a++)}return O.currentNode=z,s}p(e){let t=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(e,r,t),t+=r.strings.length-2):r._$AI(e[t])),t++}},re=class n{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(e,t,r,s){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=e,this._$AB=t,this._$AM=r,this.options=s,this._$Cv=s?.isConnected??!0}get parentNode(){let e=this._$AA.parentNode,t=this._$AM;return t!==void 0&&e?.nodeType===11&&(e=t.parentNode),e}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(e,t=this){e=H(this,e,t),ee(e)?e===c||e==null||e===""?(this._$AH!==c&&this._$AR(),this._$AH=c):e!==this._$AH&&e!==q&&this._(e):e._$litType$!==void 0?this.$(e):e.nodeType!==void 0?this.T(e):Lt(e)?this.k(e):this._(e)}O(e){return this._$AA.parentNode.insertBefore(e,this._$AB)}T(e){this._$AH!==e&&(this._$AR(),this._$AH=this.O(e))}_(e){this._$AH!==c&&ee(this._$AH)?this._$AA.nextSibling.data=e:this.T(z.createTextNode(e)),this._$AH=e}$(e){let{values:t,_$litType$:r}=e,s=typeof r=="number"?this._$AC(e):(r.el===void 0&&(r.el=te.createElement(lt(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===s)this._$AH.p(t);else{let i=new Se(s,this),a=i.u(this.options);i.p(t),this.T(a),this._$AH=i}}_$AC(e){let t=st.get(e.strings);return t===void 0&&st.set(e.strings,t=new te(e)),t}k(e){Ne(this._$AH)||(this._$AH=[],this._$AR());let t=this._$AH,r,s=0;for(let i of e)s===t.length?t.push(r=new n(this.O(X()),this.O(X()),this,this.options)):r=t[s],r._$AI(i),s++;s<t.length&&(this._$AR(r&&r._$AB.nextSibling,s),t.length=s)}_$AR(e=this._$AA.nextSibling,t){for(this._$AP?.(!1,!0,t);e!==this._$AB;){let r=Ze(e).nextSibling;Ze(e).remove(),e=r}}setConnected(e){this._$AM===void 0&&(this._$Cv=e,this._$AP?.(e))}},G=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(e,t,r,s,i){this.type=1,this._$AH=c,this._$AN=void 0,this.element=e,this.name=t,this._$AM=s,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(e,t=this,r,s){let i=this.strings,a=!1;if(i===void 0)e=H(this,e,t,0),a=!ee(e)||e!==this._$AH&&e!==q,a&&(this._$AH=e);else{let u=e,d,h;for(e=i[0],d=0;d<i.length-1;d++)h=H(this,u[r+d],t,d),h===q&&(h=this._$AH[d]),a||=!ee(h)||h!==this._$AH[d],h===c?e=c:e!==c&&(e+=(h??"")+i[d+1]),this._$AH[d]=h}a&&!s&&this.j(e)}j(e){e===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,e??"")}},Ce=class extends G{constructor(){super(...arguments),this.type=3}j(e){this.element[this.name]=e===c?void 0:e}},Ee=class extends G{constructor(){super(...arguments),this.type=4}j(e){this.element.toggleAttribute(this.name,!!e&&e!==c)}},Pe=class extends G{constructor(e,t,r,s,i){super(e,t,r,s,i),this.type=5}_$AI(e,t=this){if((e=H(this,e,t,0)??c)===q)return;let r=this._$AH,s=e===c&&r!==c||e.capture!==r.capture||e.once!==r.once||e.passive!==r.passive,i=e!==c&&(r===c||s);s&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,e),this._$AH=e}handleEvent(e){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,e):this._$AH.handleEvent(e)}},Te=class{constructor(e,t,r){this.element=e,this.type=6,this._$AN=void 0,this._$AM=t,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(e){H(this,e)}};var It=Ae.litHtmlPolyfillSupport;It?.(te,re),(Ae.litHtmlVersions??=[]).push("3.3.3");var ct=(n,e,t)=>{let r=t?.renderBefore??e,s=r._$litPart$;if(s===void 0){let i=t?.renderBefore??null;r._$litPart$=s=new re(e.insertBefore(X(),i),i,void 0,t??{})}return s._$AI(n),s};var Me=globalThis,L=class extends T{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let e=super.createRenderRoot();return this.renderOptions.renderBefore??=e.firstChild,e}update(e){let t=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(e),this._$Do=ct(t,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return q}};L._$litElement$=!0,L.finalized=!0,Me.litElementHydrateSupport?.({LitElement:L});var Ot=Me.litElementPolyfillSupport;Ot?.({LitElement:L});(Me.litElementVersions??=[]).push("4.2.2");var E=n=>(e,t)=>{t!==void 0?t.addInitializer(()=>{customElements.define(n,e)}):customElements.define(n,e)};var zt={attribute:!0,type:String,converter:Q,reflect:!1,hasChanged:ue},qt=(n=zt,e,t)=>{let{kind:r,metadata:s}=t,i=globalThis.litPropertyMetadata.get(s);if(i===void 0&&globalThis.litPropertyMetadata.set(s,i=new Map),r==="setter"&&((n=Object.create(n)).wrapped=!0),i.set(t.name,n),r==="accessor"){let{name:a}=t;return{set(u){let d=e.get.call(this);e.set.call(this,u),this.requestUpdate(a,d,n,!0,u)},init(u){return u!==void 0&&this.C(a,void 0,n,u),u}}}if(r==="setter"){let{name:a}=t;return function(u){let d=this[a];e.call(this,u),this.requestUpdate(a,d,n,!0,u)}}throw Error("Unsupported decorator location: "+r)};function g(n){return(e,t)=>typeof t=="object"?qt(n,e,t):((r,s,i)=>{let a=s.hasOwnProperty(i);return s.constructor.createProperty(i,r),a?Object.getOwnPropertyDescriptor(s,i):void 0})(n,e,t)}function p(n){return g({...n,state:!0,attribute:!1})}var m=class extends L{constructor(){super(...arguments);this.error=null;this.busy=!1}#r;connectedCallback(){super.connectedCallback(),this.#r=ke(()=>this.onSessionChange())}disconnectedCallback(){this.#r?.(),super.disconnectedCallback()}onSessionChange(){this.requestUpdate()}get sdk(){return Fe(this.client,this.baseUrl)}get sdkOrNull(){try{return this.sdk}catch{return null}}async run(t){this.error=null,this.busy=!0;try{return await t()}catch(r){if(r instanceof Error&&r.name==="AbortError")return;this.error=Dt(r);return}finally{this.busy=!1}}emit(t,r){this.dispatchEvent(new CustomEvent(t,{detail:r,bubbles:!0,composed:!0}))}static{this.baseStyles=$`
    :host {
      /* Fallback palette: the design system's light values, inlined. */
      --fb-card: #ffffff;
      --fb-subtle: #f8f8f7;
      --fb-hairline: #deded9;
      --fb-strong: #020202;
      --fb-body: #3d3d3d;
      --fb-muted: #656565;
      --fb-faint: #7c7c7c;
      --fb-brand: #00c896;
      --fb-selected: #e6f9f3;
      --fb-danger: #b3261e;

      display: block;
      font: var(--type-body, 400 15px/1.45 "Geist", system-ui, sans-serif);
      letter-spacing: var(--tracking-body, -0.005em);
      color: var(--text-body, var(--fb-body));
    }

    /* Only the *fallbacks* follow the OS. Once tokens are linked, dark mode
       is the host's decision via the oa-auto / oa-dark classes, and a
       component running its own media query would sit stubbornly light
       inside a host that had deliberately forced dark. */
    @media (prefers-color-scheme: dark) {
      :host {
        --fb-card: #1a1a1a;
        --fb-subtle: #1a1a1a;
        --fb-hairline: rgba(255, 255, 255, 0.1);
        --fb-strong: #ffffff;
        --fb-body: rgba(255, 255, 255, 0.7);
        --fb-muted: rgba(255, 255, 255, 0.6);
        --fb-faint: rgba(255, 255, 255, 0.4);
        --fb-selected: #10312a;
        --fb-danger: #ff4d4d;
      }
    }

    /* ---- The SDK frame ----
       One card, capped at --sdk-max. It never assumes a page, so it drops
       into a modal, a settings pane, a phone screen or a route unchanged. */
    .panel {
      width: 100%;
      max-width: var(--sdk-max, 420px);
      background: var(--surface-card, var(--fb-card));
      border: var(--border-width, 1px) solid
        var(--border-hairline, var(--fb-hairline));
      border-radius: var(--radius-xl, 16px);
      box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.08));
      overflow: hidden;
      box-sizing: border-box;
    }

    .panel.wide {
      max-width: var(--sdk-wide, 560px);
    }

    .head {
      display: grid;
      gap: 8px;
      padding: 24px 24px 0;
    }

    .title {
      margin: 0;
      font: var(--weight-medium, 500) var(--text-xl, 24px) / 1.2
        var(--font-display, "Geist", system-ui, sans-serif);
      letter-spacing: var(--tracking-heading, -0.02em);
      color: var(--text-strong, var(--fb-strong));
    }

    .desc {
      margin: 0;
      font: var(--type-body, 400 15px/1.45 "Geist", system-ui, sans-serif);
      color: var(--text-muted, var(--fb-muted));
    }

    .body {
      display: grid;
      gap: 14px;
      padding: 24px;
    }

    /* A labelled rule, for "or" between a provider row and the rest. */
    .divider {
      display: flex;
      align-items: center;
      gap: 10px;
    }

    .divider::before,
    .divider::after {
      content: "";
      flex: 1;
      height: 1px;
      background: var(--border-hairline, var(--fb-hairline));
    }

    .caption {
      margin: 0;
      font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
      color: var(--text-faint, var(--fb-faint));
    }

    .eyebrow {
      font: var(--type-eyebrow, 500 12px/1.2 "Geist", system-ui, sans-serif);
      letter-spacing: var(--tracking-caps, 0.08em);
      text-transform: uppercase;
      color: var(--text-faint, var(--fb-faint));
    }

    /* The O monogram on a squircle. No logo was ever supplied, so the mark
       is typographic by design: legible at 16px, works in one colour, and
       re-skins from the --logo-* tokens without redrawing anything. */
    .mark {
      display: grid;
      place-items: center;
      width: 30px;
      height: 30px;
      border-radius: var(--logo-tile-radius, 0.22em);
      background: var(--logo-tile-bg, var(--fb-strong));
      color: var(--logo-tile-fg, var(--fb-brand));
      font: var(--weight-medium, 500) 18px / 1
        var(--font-display, "Geist", system-ui, sans-serif);
      letter-spacing: var(--logo-tracking, -0.04em);
      user-select: none;
    }

    /* A value the user is meant to copy: an address, an invoice, a link.
       Shared because three elements render one and a fourth will. */
    .payload {
      display: block;
      overflow-wrap: anywhere;
      padding: 0.6em;
      border: var(--border-width, 1px) solid
        var(--border-hairline, var(--fb-hairline));
      border-radius: var(--radius-lg, 12px);
      background: var(--bg-subtle, var(--fb-subtle));
      font: var(--type-mono, 400 13px/1.5 ui-monospace, monospace);
      color: var(--text-body, var(--fb-body));
    }

    /* ---- Badge ----
       A read-only status pill. Never given a click handler: a badge that
       does something is a control wearing a status's clothes. */
    .badge {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 22px;
      padding: 0 8px;
      border-radius: var(--radius-full, 999px);
      font: var(--weight-medium, 500) var(--text-xs, 12px) / 1
        var(--font-sans, "Geist", system-ui, sans-serif);
      white-space: nowrap;
      background: var(--bg-sunken, #f2f2f2);
      color: var(--text-muted, var(--fb-muted));
    }

    .badge.success {
      background: var(--success-bg, #e6f9f3);
      color: var(--success-fg, #0b8a68);
    }

    .badge.danger {
      background: var(--danger-bg, #fdecea);
      color: var(--danger-fg, var(--fb-danger));
    }

    .badge.brand {
      background: var(--brand-soft, #e6f9f3);
      color: var(--text-brand, var(--fb-brand));
    }

    /* Inherits the pill's colour, so a tone change carries the dot with it. */
    .badge .dot {
      width: 5px;
      height: 5px;
      border-radius: var(--radius-full, 999px);
      background: currentColor;
    }

    /* ---- Field ----
       Label, control, hint. Focus is a near-black border plus the offset
       ring — never a coloured glow, and never a removed outline. */
    .field {
      display: grid;
      gap: 6px;
    }

    .field label {
      font: var(--type-ui, 500 13px/1.3 "Geist", system-ui, sans-serif);
      color: var(--text-strong, var(--fb-strong));
    }

    .field input {
      width: 100%;
      box-sizing: border-box;
      min-height: var(--control-h-md, 36px);
      padding: 0 12px;
      border: var(--border-width, 1px) solid
        var(--border-hairline, var(--fb-hairline));
      border-radius: var(--radius-md, 8px);
      background: var(--surface-card, var(--fb-card));
      color: var(--text-strong, var(--fb-strong));
      font: var(--type-body, 400 15px/1.45 "Geist", system-ui, sans-serif);
      letter-spacing: inherit;
    }

    .field input::placeholder {
      color: var(--text-faint, var(--fb-faint));
    }

    .field input:focus-visible {
      border-color: var(--text-strong, var(--fb-strong));
      box-shadow: var(--focus-ring, 0 0 0 2px #f2f2f2, 0 0 0 4px #020202);
    }

    .field .hint {
      font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
      color: var(--text-faint, var(--fb-faint));
    }

    /* Anything numeric — balances, prices, per-unit rates, ledger amounts. */
    .mono {
      font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace);
      font-variant-numeric: tabular-nums;
    }

    /* ---- Controls ----
       Height comes from the platform tokens, which re-point themselves under
       a pointer:coarse media query, so touch targets clear 44px with no
       mobile-specific variant here. */
    button {
      font: var(--type-ui, 500 13px/1.3 "Geist", system-ui, sans-serif);
      letter-spacing: inherit;
      cursor: pointer;
      min-height: var(--control-h-md, 36px);
      padding: 0 14px;
      border-radius: var(--radius-md, 8px);
      border: var(--border-width, 1px) solid
        var(--border-hairline, var(--fb-hairline));
      background: var(--surface-card, var(--fb-card));
      color: var(--text-strong, var(--fb-strong));
      transition: var(--transition-control, all 120ms cubic-bezier(0.2, 0, 0, 1));
    }

    button:hover:not([disabled]) {
      background: var(--surface-hover, rgba(0, 0, 0, 0.04));
    }

    /* The whole press feedback is half a pixel. Nothing scales. */
    button:active:not([disabled]) {
      transform: translateY(0.5px);
    }

    button.primary {
      background: var(--brand, var(--fb-brand));
      border-color: var(--brand, var(--fb-brand));
      color: var(--brand-contrast, #020202);
    }

    button.primary:hover:not([disabled]) {
      background: var(--brand-soft, #1ad3a5);
    }

    button.ghost {
      background: transparent;
      border-color: transparent;
    }

    button.block {
      width: 100%;
      justify-content: center;
    }

    button[disabled] {
      opacity: 0.45;
      cursor: not-allowed;
    }

    /* A ring, never a glow, and never removed. */
    :focus-visible {
      outline: none;
      box-shadow: var(--focus-ring, 0 0 0 2px #f2f2f2, 0 0 0 4px #020202);
    }

    a {
      color: var(--text-link, var(--fb-strong));
      text-decoration: none;
      text-underline-offset: 3px;
    }

    a:hover {
      text-decoration: underline;
    }

    .error {
      color: var(--danger-fg, var(--fb-danger));
      font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
      margin-top: 0.5em;
    }

    .muted {
      color: var(--text-muted, var(--fb-muted));
    }
  `}};l([g({type:String,attribute:"base-url"})],m.prototype,"baseUrl",2),l([g({attribute:!1})],m.prototype,"client",2),l([p()],m.prototype,"error",2),l([p()],m.prototype,"busy",2);function Dt(n){if(n instanceof y)switch(n.code){case"unauthorized":return"Please sign in again.";case"insufficient_balance":return n.balance===void 0?"Not enough credits.":`Not enough credits \u2014 you have ${n.balance}.`;case"rate_limited":return"Too many attempts. Please wait a moment and try again.";case"network":return"Could not reach the server. Check your connection.";case"timeout":return"This is taking longer than expected. It may still complete.";default:return n.message}return n instanceof Error?n.message:String(n)}var dt=pe`<svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true" focusable="false"><path fill="#4285F4" d="M17.64 9.205c0-.639-.057-1.252-.164-1.841H9v3.481h4.844a4.14 4.14 0 0 1-1.796 2.716v2.258h2.909c1.702-1.567 2.683-3.874 2.683-6.614z"/><path fill="#34A853" d="M9 18c2.43 0 4.467-.806 5.956-2.181l-2.909-2.258c-.806.54-1.837.859-3.047.859-2.344 0-4.328-1.583-5.036-3.71H.957v2.332A8.997 8.997 0 0 0 9 18z"/><path fill="#FBBC05" d="M3.964 10.71A5.41 5.41 0 0 1 3.682 9c0-.593.102-1.17.282-1.71V4.958H.957A8.996 8.996 0 0 0 0 9c0 1.452.348 2.827.957 4.042l3.007-2.332z"/><path fill="#EA4335" d="M9 3.58c1.321 0 2.508.454 3.44 1.346l2.582-2.582C13.463.892 11.426 0 9 0A8.997 8.997 0 0 0 .957 4.958L3.964 7.29C4.672 5.163 6.656 3.58 9 3.58z"/></svg>`,ut=pe`<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false"><g fill="#627EEA"><path d="M12 1.5 5.75 12.02 12 15.73V1.5z" opacity=".55"/><path d="M12 1.5v14.23l6.25-3.71L12 1.5z" opacity=".85"/><path d="M12 17.06 5.75 13.35 12 22.5v-5.44z" opacity=".55"/><path d="M12 22.5v-5.44l6.25-3.71L12 22.5z" opacity=".85"/></g></svg>`,ht=pe`<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false"><path fill="#9C59FF" d="M3 23.9C2.7 23.6 2.8 22.7 3.3 22.1C3.4 21.9 3.4 21.9 3.2 21.9C3 22 2.9 21.9 2.9 21.6C3 21.3 3.4 21 3.9 20.9C4.4 20.8 4.4 20.8 5.2 19.5C5.7 18.9 6.3 18.1 6.5 17.7C6.9 17.2 7 17 7.1 16.8C7.2 16.3 7.4 16 7.9 15.8C8.5 15.6 10.4 13.8 10.2 13.6C10.2 13.6 10 13.6 9.7 13.5C8.7 13.3 7.5 12.8 6.9 12.4C6.6 12.2 6.6 12.2 6.4 12.2C5.6 12.3 4.9 12.5 4.4 12.8C3.8 13.1 3.8 13.2 3.7 13.1C3.6 13 3.6 12.4 3.8 12C3.8 11.9 3.8 11.9 3.7 11.9C3.6 11.9 3.4 12 3.1 12.3C2.7 12.8 2.6 12.8 2.5 12.6C2.3 12.4 2.4 12.1 2.5 11.7C2.6 11.2 2.6 11.2 2.5 11.2C2.4 11.3 2.2 11.4 2 11.4C1.6 11.6 1.5 11.6 1.5 11.4C1.5 10.7 2.3 9.7 3 9.3C3.7 8.9 4.9 8.8 5.2 9C5.3 9.1 5.6 9.2 5.6 9.2C5.6 9.2 5.6 9.1 5.5 9C5.4 8.8 5.4 8.6 5.5 8.6C5.5 8.6 5.9 8.6 6.3 8.6C7.6 8.6 8.1 8.4 9.4 7.8C10.9 7.1 11.1 7 11.7 6.8C12.5 6.5 12.9 6.4 13.9 6.4C15.4 6.3 16 6.5 17.4 7.3C18 7.7 18.1 7.7 18.4 7.6C18.7 7.6 18.8 7.6 19.1 7.6C19.7 7.7 20 7.7 20.4 7.5C21.1 7.1 21.4 6.5 21.3 5.7C21.3 5 21.1 4.7 20.2 4.1C19.1 3.2 18.7 2.5 18.7 1.5C18.7 0.9 18.8 0.6 19.1 0.3C19.5 -0.1 19.9 -0.1 20.6 0.4C21 0.6 21.2 0.7 21.8 0.9C22.6 1.2 22.7 1.2 22.2 1.3C21.8 1.3 21.8 1.3 22.1 1.4C22.7 1.6 22.6 1.7 21.7 1.7C21.1 1.7 20.9 1.7 20.6 1.8C20.1 1.9 20 2 20.1 2.2C20.1 2.5 20.2 2.6 20.9 3.1C22.1 4.1 22.5 4.8 22.5 6C22.4 7.5 21.5 8.7 19.8 9.8C19.2 10.1 19.2 10.1 19.2 10.7C19.2 11.9 19 12.5 18.3 13.1C17.5 13.7 16.6 13.9 15.1 14L14.3 14L14.2 14.2C14.1 14.2 14.1 14.3 14.1 14.4C14.1 14.4 13.8 14.6 13.5 14.8C13.2 15 12.6 15.8 12.9 15.7C12.9 15.7 13.4 15.5 14 15.3C17 14.4 16.7 14.5 17.2 14.5C17.8 14.5 17.8 14.5 18.4 15.4C19 16.3 19.1 16.5 19 16.6C19 16.8 18.5 16.6 18 16.1C17.7 15.8 17.6 15.8 17.7 16.1C17.7 16.4 17.6 16.5 17.4 16.4C17.3 16.3 17.2 16.2 17.1 15.8L17.1 15.5L16.9 15.5C16.6 15.5 16.6 15.5 14.5 16.2C13.3 16.6 12.9 16.7 12.7 16.9C12 17.2 11.5 17 11.5 16.3C11.5 16.1 11.9 14.9 12.1 14.8C12.1 14.8 12.3 14.4 12.2 14.4C12.2 14.4 11.9 14.5 11.6 14.6L11 14.8L10 15.6C9 16.4 9 16.4 8.9 16.6C8.8 17 8.5 17.3 8.1 17.4C7.9 17.5 7.8 17.7 6.9 18.8C5.9 20.1 5.3 20.9 4.9 21.6C4.8 21.8 4.5 22.1 4.3 22.4C3.7 22.9 3.6 23.1 3.3 23.6C3.1 24 3.1 24 3 23.9Z"/></svg>`;var v=class extends Error{constructor(e){super(e),this.name="WalletError"}};function pt(){return typeof window>"u"?[]:[{where:"window.nostr",provider:window.nostr},{where:"window.okxwallet.nostr",provider:window.okxwallet?.nostr}]}function Ht(n){let e=n;return!!e&&typeof e.getPublicKey=="function"&&typeof e.signEvent=="function"}function Le(){for(let{provider:n}of pt())if(Ht(n))return n;return null}async function me(n=2e3){let e=Date.now()+n;for(;;){let t=Le();if(t)return t;let r=e-Date.now();if(r<=0)return null;await new Promise(s=>setTimeout(s,Math.min(100,r)))}}function Ue(){return pt().map(n=>n.where)}function Gt(n){if(!n)return"wallet";let e=n;return typeof e.__name=="string"?e.__name:e.isOkxWallet||e.isOKExWallet?"OKX Wallet":e.isRabby?"Rabby":e.isBraveWallet?"Brave Wallet":e.isCoinbaseWallet?"Coinbase Wallet":e.isMetaMask?"MetaMask":"wallet"}function Ie(){if(typeof window>"u")return null;for(let n of[window.ethereum,window.okxwallet])if(n&&typeof n.request=="function")return n;return null}function Wt(){return Ie()!==null}function jt(){return Le()!==null}function Bt(){let n=[];return Wt()&&n.push("eip155"),jt()&&n.push("nostr"),n}async function Oe(n,e){let t;try{t=JSON.parse(n)}catch{throw new v("server sent an unreadable Nostr challenge")}let{nip19:r,finalizeEvent:s}=await import("./esm-ZDSEP2UJ.js"),i;try{let a=r.decode(e.trim());if(a.type!=="nsec")throw new v(`that is an ${a.type} key \u2014 sign-in needs the secret key, which starts with nsec1`);i=a.data}catch(a){throw a instanceof v?a:new v("that does not look like a valid nsec1\u2026 key")}try{let a=s({kind:t.kind,content:t.content,tags:t.tags,created_at:t.created_at??Math.floor(Date.now()/1e3)},i);return{type:"nostr_event",event:JSON.stringify(a)}}finally{i.fill(0)}}async function W(){let n=Ie();if(!n)throw new v("no Ethereum wallet found in this browser");let e=Gt(n),t;try{t=await n.request({method:"eth_requestAccounts"})}catch(s){throw new v(qe(s,`${e} rejected the connection`))}let r=Array.isArray(t)?t[0]:void 0;if(typeof r!="string"||!r)throw new v("wallet returned no accounts");return r}async function j(n,e){let t=Ie();if(!t)throw new v("no Ethereum wallet found in this browser");try{let r=await t.request({method:"personal_sign",params:[n,e]});if(typeof r!="string")throw new v("wallet returned no signature");return{type:"signature",signature:r}}catch(r){throw r instanceof v?r:new v(qe(r,"signature was rejected"))}}async function B(n){let e=await me();if(!e)throw new v(`no Nostr signer answered (looked at ${Ue().join(", ")})`);let t;try{t=JSON.parse(n)}catch{throw new v("server sent an unreadable Nostr challenge")}t.created_at??=Math.floor(Date.now()/1e3);try{let r=await e.signEvent(t);return{type:"nostr_event",event:JSON.stringify(r)}}catch(r){throw new v(qe(r,"signing was rejected"))}}var Ft=6e4;async function ze(n,e,t={}){let r;try{r=JSON.parse(n)}catch{throw new v("server sent an unreadable Nostr challenge")}let[{BunkerSigner:s,parseBunkerInput:i},{generateSecretKey:a}]=await Promise.all([import("./nip46-PMGLFUAT.js"),import("./pure-F6KPRDZ5.js")]),u=await i(e.trim()).catch(()=>null);if(!u)throw new v("that is not a bunker:// address or a NIP-05 name \u2014 copy the connection string from your signer app");let d=s.fromBunker(a(),u,{onauth:h=>t.onAuthUrl?.(h)});try{let h=await Vt((async()=>(await d.connect(),d.signEvent({kind:r.kind,content:r.content,tags:r.tags,created_at:r.created_at??Math.floor(Date.now()/1e3)})))(),t.timeoutMs??Ft,"the signer did not respond \u2014 check it is running and try again");return{type:"nostr_event",event:JSON.stringify(h)}}catch(h){throw h instanceof v?h:new v(h instanceof Error?h.message:"the remote signer refused")}finally{await d.close().catch(()=>{})}}function Vt(n,e,t){return new Promise((r,s)=>{let i=setTimeout(()=>s(new v(t)),e);n.then(a=>{clearTimeout(i),r(a)},a=>{clearTimeout(i),s(a)})})}function qe(n,e){if(n&&typeof n=="object"){let t=n;if(t.code===4001)return e;if(t.message)return t.message}return e}var w=class extends m{constructor(){super(...arguments);this.me=null;this.enabled=null;this.signerTimeout=2e3;this.variant="inline";this.heading="Sign in to OpenApps";this.description="One account for every app in the suite. Optional \u2014 the apps work without it.";this.mark="O";this.nostrFallback="none";this.nostrHint=null;this.authUrl=null}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){let t=await this.run(()=>this.sdk.auth.completeRedirect());if(t&&(R(),this.emit("openapps-login",t),k()),this.enabled||(this.enabled=await this.run(()=>this.sdk.auth.methods())??null),!this.sdk.isLoggedIn){this.me=null;return}this.me=await this.run(()=>this.sdk.auth.me())??null}async loginWithWallet(){await this.run(async()=>{let t=await W(),r=await this.sdk.auth.challenge("eip155",t),s=await j(r.message,t),i=await this.sdk.auth.verify(r.challenge_id,s,{referralCode:ne()});R(),this.emit("openapps-login",i),k()})}async loginWithNostr(){if(!await me(this.signerTimeout)){this.nostrFallback="bunker",this.nostrHint=`No signer extension answered. Checked ${Ue().join(" and ")}. On a phone, or without an extension, connect a remote signer below.`;return}await this.run(async()=>{let t=await this.sdk.auth.challenge("nostr"),r=await B(t.message),s=await this.sdk.auth.verify(t.challenge_id,r,{referralCode:ne()});R(),this.emit("openapps-login",s),k()})}async loginWithBunker(t){t.preventDefault();let s=this.renderRoot.querySelector("#bunker")?.value.trim()??"";s&&(this.authUrl=null,await this.run(async()=>{let i=await this.sdk.auth.challenge("nostr"),a=await ze(i.message,s,{onAuthUrl:d=>{this.authUrl=d}}),u=await this.sdk.auth.verify(i.challenge_id,a,{referralCode:ne()});this.nostrFallback="none",this.authUrl=null,R(),this.emit("openapps-login",u),k()}))}async loginWithNsec(t){t.preventDefault();let r=this.renderRoot.querySelector("#nsec"),s=r?.value.trim()??"";s&&await this.run(async()=>{try{let i=await this.sdk.auth.challenge("nostr"),a=await Oe(i.message,s),u=await this.sdk.auth.verify(i.challenge_id,a,{referralCode:ne()});this.nostrFallback="none",R(),this.emit("openapps-login",u),k()}finally{r&&(r.value="")}})}loginWithGoogle(){let t=`${location.origin}${location.pathname}${location.search}`;window.location.href=this.sdk.auth.googleStartUrl(t,ne())}async logout(){await this.run(()=>this.sdk.auth.logout()),this.me=null,this.emit("openapps-logout",null),k()}render(){if(this.me)return this.renderSignedIn(this.me);let t=this.enabled?.google??!1,r=this.enabled?.eip155??!1,s=this.enabled?.nostr??!1;if(this.enabled&&!t&&!r&&!s)return this.frame(o`
        <p class="muted">This server has no login methods configured.</p>
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
      `);let i=this.variant==="panel"?"block":"";return this.frame(o`
      <div class="stack">
        ${t?o`<button
              class="provider ${i}"
              ?disabled=${this.busy}
              @click=${this.loginWithGoogle}
            >
              ${dt}<span>Continue with Google</span>
            </button>`:c}
        ${r?o`<button
              class="provider ${i}"
              ?disabled=${this.busy}
              @click=${this.loginWithWallet}
            >
              ${ut}<span>Continue with a wallet</span>
            </button>`:c}
        ${s?o`
              <button
                class="provider ${i}"
                ?disabled=${this.busy}
                @click=${this.loginWithNostr}
              >
                ${ht}<span>Continue with Nostr</span>
              </button>
              ${this.renderNostrFallback()}
            `:c}
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
      </div>
    `)}frame(t){return this.variant!=="panel"?t:o`
      <div class="panel">
        <div class="head">
          <span class="mark" aria-hidden="true">${this.mark}</span>
          <h1 class="title">${this.heading}</h1>
          ${this.description?o`<p class="desc">${this.description}</p>`:c}
        </div>
        <div class="body">${t}</div>
      </div>
    `}renderNostrFallback(){return this.nostrFallback==="bunker"?this.renderBunkerForm():this.nostrFallback==="nsec"?this.renderNsecForm():o`
      <button
        class="link"
        ?disabled=${this.busy}
        @click=${()=>{this.nostrFallback="bunker",this.nostrHint=null}}
      >
        No extension? Use a remote signer
      </button>
    `}renderBunkerForm(){return o`
      <form class="nsec" @submit=${this.loginWithBunker}>
        ${this.nostrHint?o`<p class="muted small">${this.nostrHint}</p>`:c}
        <p class="muted small">
          Paste the connection string from your signer — Amber, nsec.app, or your
          own bunker. It looks like <code>bunker://…</code>, or you can use a
          NIP-05 name. <strong>Your key never leaves the signer</strong>; only the
          request to sign travels, and you approve it there.
        </p>
        <div class="field">
          <label for="bunker">Remote signer</label>
          <input
            id="bunker"
            type="text"
            placeholder="bunker://… or you@example.com"
            autocomplete="off"
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            ?disabled=${this.busy}
          />
          <span class="hint">
            A bunker URI, or the NIP-05 address your signer is reachable at.
          </span>
        </div>
        ${this.authUrl?o`<p class="muted small">
              Your signer needs approval:
              <a href=${this.authUrl} target="_blank" rel="noreferrer noopener"
                >open it</a
              >, then come back.
            </p>`:c}
        <div class="row">
          <button class="primary" type="submit" ?disabled=${this.busy}>
            ${this.busy?"Waiting for your signer\u2026":"Connect signer"}
          </button>
          <button
            type="button"
            ?disabled=${this.busy}
            @click=${()=>this.nostrFallback="none"}
          >
            Cancel
          </button>
        </div>
        <button
          class="link"
          type="button"
          ?disabled=${this.busy}
          @click=${()=>{this.nostrFallback="nsec",this.nostrHint=null}}
        >
          I only have a private key
        </button>
      </form>
    `}renderNsecForm(){return o`
      <form class="nsec" @submit=${this.loginWithNsec}>
        ${this.nostrHint?o`<p class="muted small">${this.nostrHint}</p>`:c}
        <p class="warn">
          <strong>Only do this on a key you can afford to lose.</strong>
          An <code>nsec</code> is your whole Nostr identity — it cannot be changed
          without abandoning the account, and any script on this page can read it
          while you paste it. A signer extension exists so a website never sees
          your key.
        </p>
        <div class="field">
          <label for="nsec">Secret key</label>
          <input
            id="nsec"
            type="password"
            placeholder="nsec1…"
            autocomplete="off"
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            ?disabled=${this.busy}
          />
        </div>
        <p class="muted small">
          The key is used in this browser to sign one message. It is not sent to
          the server and not saved anywhere.
        </p>
        <div class="row">
          <button class="primary" type="submit" ?disabled=${this.busy}>
            Sign in
          </button>
          <button type="button" ?disabled=${this.busy} @click=${()=>this.nostrFallback="none"}>
            Cancel
          </button>
        </div>
      </form>
    `}renderSignedIn(t){let r=t.display_name??t.linked_accounts[0]?.caip10??t.id;return o`
      <div class="row">
        <span class="identity" title=${r}>${Kt(r)}</span>
        <button ?disabled=${this.busy} @click=${this.logout}>Sign out</button>
      </div>
      ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}};w.styles=[m.baseStyles,$`
      /* Mark on the left, label centred in the space that remains — so the
         three labels line up with each other rather than each sitting a
         different distance from its own icon. */
      button.provider {
        display: flex;
        align-items: center;
        gap: 10px;
        text-align: left;
      }
      button.provider svg {
        flex: none;
      }
      button.provider.block span {
        flex: 1;
        text-align: center;
        /* Balance the mark so the label is centred on the button, not on
           the space beside the mark. */
        margin-right: 26px;
      }
      .stack {
        display: flex;
        flex-direction: column;
        gap: 0.5em;
      }
      .row {
        display: flex;
        align-items: center;
        gap: 0.75em;
      }
      .identity {
        font-variant-numeric: tabular-nums;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      button.link {
        border: none;
        background: none;
        color: var(--text-muted, var(--fb-muted));
        text-decoration: underline;
        padding: 0.1em 0;
        font-size: 0.85em;
        text-align: left;
      }
      .nsec {
        display: flex;
        flex-direction: column;
        gap: 0.5em;
        border: 1px solid var(--border-hairline, var(--fb-hairline));
        border-radius: var(--radius-lg, 12px);
        padding: 0.8em;
      }
      /* Everything else comes from .field. Only the family is overridden:
         a key is a literal, and mono carries literals. */
      .nsec input {
        font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace);
      }
      /* Semantic tokens, so a host forcing dark gets the dark warning —
         the hardcoded pair here used its own media query and so ignored
         the host entirely. */
      .warn {
        font-size: 0.8rem;
        margin: 0;
        padding: 0.5em 0.6em;
        border-radius: var(--radius-lg, 12px);
        background: var(--warning-bg, #fef3c7);
        color: var(--warning-fg, #92400e);
      }
      .small {
        font-size: 0.78rem;
      }
      .row {
        display: flex;
        gap: 0.5em;
      }
    `],l([p()],w.prototype,"me",2),l([p()],w.prototype,"enabled",2),l([g({type:Number,attribute:"signer-timeout"})],w.prototype,"signerTimeout",2),l([g({type:String})],w.prototype,"variant",2),l([g({type:String})],w.prototype,"heading",2),l([g({type:String})],w.prototype,"description",2),l([g({type:String})],w.prototype,"mark",2),l([p()],w.prototype,"nostrFallback",2),l([p()],w.prototype,"nostrHint",2),l([p()],w.prototype,"authUrl",2),w=l([E("openapps-login")],w);function Kt(n,e=10,t=6){return n.length<=e+t+1?n:`${n.slice(0,e)}\u2026${n.slice(-t)}`}function ne(){return oe()??ye()}var De={google:"Google",eip155:"Wallet",nostr:"Nostr"},P=class extends m{constructor(){super(...arguments);this.me=null;this.enabled=null;this.pending=null;this.notice=null;this.blocked=null}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){if(await Promise.resolve(),this.handleLinkRedirect(),this.enabled||(this.enabled=await this.run(()=>this.sdk.auth.methods())??null),!this.sdk.isLoggedIn){this.me=null;return}this.me=await this.run(()=>this.sdk.auth.me())??null}linked(t){return(this.me?.linked_accounts??[]).some(r=>r.namespace===t)}get connectable(){return["eip155","nostr"].filter(t=>this.enabled?.[t]&&!this.linked(t))}get canConnectGoogle(){return(this.enabled?.google??!1)&&!this.linked("google")}async signOut(){await this.run(()=>this.sdk.auth.logout()),this.me=null,this.emit("openapps-logout",null),k()}async connectGoogle(t=!1){await this.run(async()=>{let r=`${location.origin}${location.pathname}${location.search}`,s=await this.sdk.auth.googleLinkStart(r,{merge:t});window.location.href=s})}handleLinkRedirect(){let t;try{t=this.sdk.auth.completeLinkRedirect()}catch{return}if(t)switch(t.status){case"linked":this.notice=t.merged?`Accounts combined \u2014 ${t.credits.toLocaleString()} credits moved across.`:"Google connected.",this.emit("openapps-identity-linked",t),k();break;case"conflict":this.pending={namespace:"google",other:{id:"",balance:t.balance}};break;case"blocked":this.blocked=t.message;break;case"error":this.error=t.message;break}}async connect(t){this.blocked=null,await this.run(async()=>{let r=t==="eip155"?await W():void 0,s=await this.sdk.auth.linkChallenge(t,r),i=t==="eip155"?await j(s.message,r):await B(s.message);try{let a=await this.sdk.auth.linkVerify(s.challenge_id,i);this.afterLink(a)}catch(a){if(a instanceof y&&(a.detail?.code==="merge_blocked_by_duplicate_namespace"||a.detail?.code==="namespace_already_linked")){this.blocked=a.message;return}if(a instanceof y&&a.detail?.code==="identity_belongs_to_another_account"){this.pending={namespace:t,other:a.detail.other_account};return}throw a}})}async confirmMerge(){let t=this.pending;if(t){if(t.namespace==="google"){this.pending=null,await this.connectGoogle(!0);return}await this.run(async()=>{let r=t.namespace==="eip155"?await W():void 0,s=await this.sdk.auth.linkChallenge(t.namespace,r),i=t.namespace==="eip155"?await j(s.message,r):await B(s.message),a=await this.sdk.auth.linkVerify(s.challenge_id,i,{merge:!0});this.pending=null,this.afterLink(a)})}}afterLink(t){this.notice=t.merged?`Accounts combined \u2014 ${(t.credits_transferred??0).toLocaleString()} credits moved across.`:"Connected.",this.emit("openapps-identity-linked",t),k(),this.load()}async unlink(t){await this.run(async()=>{await this.sdk.auth.unlink(t),this.notice="Disconnected.",this.emit("openapps-identity-unlinked",{caip10:t}),await this.load()})}render(){if(!this.sdkOrNull)return o`<p class="muted">Loading…</p>`;if(!this.sdk.isLoggedIn)return o`<p class="muted">Sign in to manage your account.</p>`;if(!this.me)return o`<p class="muted">Loading…</p>`;if(this.pending)return this.renderMergePrompt(this.pending);let t=this.me.linked_accounts;return o`
      <div class="card">
        <div class="head">
          <div>
            <div class="muted small">Account</div>
            <code class="id">${this.me.id}</code>
          </div>
          <div class="right">
            <div class="balance">${this.me.balance.toLocaleString()}</div>
            <div class="muted small">credits</div>
          </div>
        </div>

        <h3>Sign-in methods</h3>
        <ul class="identities">
          ${t.map(r=>o`
              <li>
                <span class="tag">${De[r.namespace]??r.namespace}</span>
                <code title=${r.caip10}
                  >${Jt(r.label??r.caip10)}</code
                >
                ${t.length>1?o`<button
                      class="link"
                      ?disabled=${this.busy}
                      @click=${()=>this.unlink(r.caip10)}
                    >
                      Disconnect
                    </button>`:o`<span class="muted small">only method</span>`}
              </li>
            `)}
        </ul>

        ${this.connectable.length||this.canConnectGoogle?o`
              <h3>Add another</h3>
              <div class="row">
                ${this.canConnectGoogle?o`<button ?disabled=${this.busy} @click=${()=>this.connectGoogle()}>
                      Connect Google
                    </button>`:c}
                ${this.connectable.map(r=>o`
                    <button ?disabled=${this.busy} @click=${()=>this.connect(r)}>
                      Connect ${De[r]}
                    </button>
                  `)}
              </div>
              <p class="muted small">
                Connecting a method that is already on another account will offer to
                combine them, so you keep one balance and one history.
              </p>
            `:o`<p class="muted small">Every available method is connected.</p>`}

        ${this.blocked?o`<p class="warn" role="alert">${this.blocked}</p>`:c}
        ${this.notice?o`<p class="notice">${this.notice}</p>`:c}
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
      </div>

      <button class="signout" ?disabled=${this.busy} @click=${this.signOut}>
        Sign out
      </button>
    `}renderMergePrompt(t){return o`
      <div class="card">
        <h3>Combine two accounts?</h3>
        <p>
          That ${De[t.namespace]??t.namespace} identity already
          belongs to another account holding
          <strong>${t.other.balance.toLocaleString()} credits</strong>.
        </p>
        <p class="muted small">
          Combining moves its credits, payment history and referral earnings onto
          this account, and signs it out everywhere. It cannot be undone.
        </p>
        <div class="row">
          <button class="primary" ?disabled=${this.busy} @click=${this.confirmMerge}>
            Combine them
          </button>
          <button ?disabled=${this.busy} @click=${()=>this.pending=null}>
            Cancel
          </button>
        </div>
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}};P.styles=[m.baseStyles,$`
      .card {
        border: 1px solid var(--border-hairline, var(--fb-hairline));
        border-radius: var(--radius-lg, 12px);
        padding: 1rem 1.1rem;
        background: var(--surface-card, var(--fb-card));
      }
      .head {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 1rem;
        border-bottom: 1px solid var(--border-hairline, var(--fb-hairline));
        padding-bottom: 0.75rem;
      }
      .right {
        text-align: right;
      }
      /* Below the card, full width, quiet — the same shape and place
         OpenCapture arrived at independently when it had to build this
         itself. Bottom is where a finished-with action belongs: nobody
         opens an account panel in order to sign out, and putting it
         beside the balance makes the one control that throws a session
         away the easiest one to reach by accident. */
      .signout {
        display: block;
        width: 100%;
        margin-top: 16px;
        font: inherit;
        font-size: 0.875rem;
        padding: 9px 12px;
        border-radius: var(--radius-md, 8px);
        border: 1px solid var(--border-hairline, var(--fb-hairline));
        background: transparent;
        color: var(--text-muted, var(--fb-muted));
        cursor: pointer;
      }
      .signout:hover:not(:disabled) {
        color: var(--text-strong, var(--fb-strong));
        border-color: var(--text-muted, var(--fb-muted));
      }
      .signout:disabled {
        opacity: 0.5;
        cursor: default;
      }
      .balance {
        font-size: 1.5rem;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
      }
      .id {
        font-size: 0.8rem;
        overflow-wrap: anywhere;
      }
      h3 {
        font-size: 0.9rem;
        margin: 1rem 0 0.5rem;
      }
      .identities {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
      }
      .identities li {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        font-size: 0.9rem;
      }
      .identities code {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .tag {
        font-size: 0.7rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        border: 1px solid var(--border-hairline, var(--fb-hairline));
        border-radius: 999px;
        padding: 0.1em 0.6em;
      }
      button.link {
        border: none;
        background: none;
        color: var(--text-muted, var(--fb-muted));
        text-decoration: underline;
        padding: 0;
        font-size: 0.85em;
      }
      .row {
        display: flex;
        gap: 0.5rem;
        flex-wrap: wrap;
      }
      .small {
        font-size: 0.8rem;
      }
      .notice {
        font-size: 0.85rem;
        margin-top: 0.6rem;
      }
      /* Semantic tokens rather than a hardcoded pair with its own media
         query: that combination ignored the host entirely, so a page
         forcing dark kept a bright yellow warning. */
      .warn {
        font-size: 0.85rem;
        margin-top: 0.6rem;
        padding: 0.5em 0.7em;
        border-radius: var(--radius-lg, 12px);
        background: var(--warning-bg, #fef3c7);
        color: var(--warning-fg, #92400e);
      }
    `],l([p()],P.prototype,"me",2),l([p()],P.prototype,"enabled",2),l([p()],P.prototype,"pending",2),l([p()],P.prototype,"notice",2),l([p()],P.prototype,"blocked",2),P=l([E("openapps-account")],P);function Jt(n,e=18,t=8){return n.length<=e+t+1?n:`${n.slice(0,e)}\u2026${n.slice(-t)}`}var F,A=class extends m{constructor(){super(...arguments);this.pollSeconds=0;this.label="Credits";this.balance=null;ie(this,F)}connectedCallback(){super.connectedCallback(),this.refresh(),this.pollSeconds>0&&ae(this,F,setInterval(()=>{this.refresh()},this.pollSeconds*1e3))}disconnectedCallback(){U(this,F)&&clearInterval(U(this,F)),super.disconnectedCallback()}onSessionChange(){this.refresh()}async refresh(){let t=this.sdkOrNull;if(!t?.isLoggedIn){this.balance=null;return}let r=await this.run(()=>t.credits.balance());r!==void 0&&(this.balance=r)}render(){return this.sdkOrNull?this.sdk.isLoggedIn?o`
      <span class="wrap">
        <span class="label muted">${this.label}</span>
        <span class="value" aria-live="polite"
          >${this.balance===null?"\u2026":this.balance.toLocaleString()}</span
        >
      </span>
      ${this.error?o`<span class="error" role="alert">${this.error}</span>`:c}
    `:o`<span class="muted">Not signed in</span>`:o`<span class="muted">…</span>`}};F=new WeakMap,A.styles=[m.baseStyles,$`
      :host {
        display: inline-block;
      }
      .wrap {
        display: inline-flex;
        align-items: baseline;
        gap: 0.4em;
      }
      .label {
        font: var(--type-eyebrow, 500 12px/1.2 "Geist", system-ui, sans-serif);
        letter-spacing: var(--tracking-caps, 0.08em);
        text-transform: uppercase;
        color: var(--text-faint, var(--fb-faint));
      }
      /* A balance is a quantity, so it is set in mono. Tabular figures also
         stop the number jittering sideways as it ticks over on poll. */
      .value {
        font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace);
        font-variant-numeric: tabular-nums;
        font-weight: var(--weight-medium, 500);
        color: var(--text-strong, var(--fb-strong));
      }
    `],l([g({type:Number,attribute:"poll-seconds"})],A.prototype,"pollSeconds",2),l([g({type:String})],A.prototype,"label",2),l([p()],A.prototype,"balance",2),A=l([E("openapps-credits")],A);var x=class extends m{constructor(){super(...arguments);this.pageSize=25;this.appId="";this.noSummary=!1;this.entries=[];this.cursor=null;this.complete=!1;this.loaded=!1}connectedCallback(){super.connectedCallback(),this.refresh()}onSessionChange(){this.refresh()}async refresh(){this.entries=[],this.cursor=null,this.complete=!1,this.loaded=!1,await this.loadMore()}async loadMore(){let t=this.sdkOrNull;if(!t?.isLoggedIn){this.loaded=!0;return}let r=await this.run(()=>t.credits.history({cursor:this.cursor??void 0,limit:this.pageSize}));this.loaded=!0,r&&(this.entries=[...this.entries,...r.entries],this.cursor=r.next_cursor,this.complete=r.next_cursor===null)}get visible(){return this.appId?this.entries.filter(t=>t.app_id===this.appId):this.entries}get spending(){let t=new Map;for(let r of this.visible){if(r.amount>=0)continue;let s=He(r),i=t.get(s);i?(i.credits+=-r.amount,i.count+=1):t.set(s,{label:s,credits:-r.amount,count:1})}return[...t.values()].sort((r,s)=>s.credits-r.credits)}render(){if(!this.sdkOrNull)return o`<p class="muted">Loading…</p>`;if(!this.sdk.isLoggedIn)return o`<p class="muted">Sign in to see where your credits went.</p>`;if(!this.loaded)return o`<p class="muted">Loading…</p>`;let t=this.visible;if(t.length===0)return o`
        <p class="muted">
          Nothing yet. Credits you buy and spend both appear here, with what
          each one was for.
        </p>
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
      `;let r=this.noSummary?[]:this.spending,s=r.reduce((i,a)=>i+a.credits,0);return o`
      ${r.length?o`
            <div class="summary">
              <div class="eyebrow">
                ${this.complete?"Spent, all time":"Spent, recent activity"}
              </div>
              <ul class="groups">
                ${r.map(i=>o`
                    <li>
                      <span class="what" title=${i.label}>${i.label}</span>
                      <span class="times muted"
                        >${i.count===1?"once":`${i.count}\xD7`}</span
                      >
                      <span class="mono amount">${i.credits.toLocaleString()}</span>
                      <!-- The bar is proportional to the largest row, not to
                           the total: with one dominant item every other bar
                           would round to nothing and the comparison people
                           actually want — this against that — disappears. -->
                      <span
                        class="bar"
                        style=${`--w:${Math.round(i.credits/r[0].credits*100)}%`}
                      ></span>
                    </li>
                  `)}
              </ul>
              <p class="caption">
                ${s.toLocaleString()} credits across ${t.length}
                ${t.length===1?"entry":"entries"}${this.complete?"":" so far"}.
              </p>
            </div>
          `:c}

      <div class="eyebrow rule">Activity</div>
      <ul class="entries">
        ${t.map(i=>o`
            <li>
              <span class="when muted">${Yt(i.created_at)}</span>
              <span class="what" title=${ft(i)}
                >${ft(i)}</span
              >
              <span class="mono amount ${i.amount<0?"debit":"credit"}">
                ${i.amount>0?"+":"\u2212"}${Math.abs(i.amount).toLocaleString()}
              </span>
              <span class="mono after muted" title="Balance after"
                >${i.balance_after.toLocaleString()}</span
              >
            </li>
          `)}
      </ul>

      ${this.cursor!==null?o`<button
            class="block"
            ?disabled=${this.busy}
            @click=${()=>{this.loadMore()}}
          >
            ${this.busy?"Loading\u2026":"Show earlier"}
          </button>`:c}
      ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}};x.styles=[m.baseStyles,$`
      ul {
        list-style: none;
        margin: 0;
        padding: 0;
      }
      .summary {
        display: grid;
        gap: 8px;
        margin-bottom: 18px;
      }
      /* Label, count, amount — then the bar spanning the full width beneath,
         so a long feature name never squeezes the figure it belongs to. */
      .groups li {
        display: grid;
        grid-template-columns: 1fr auto auto;
        align-items: baseline;
        gap: 8px;
        padding: 5px 0;
      }
      .groups .bar {
        grid-column: 1 / -1;
        height: 3px;
        border-radius: var(--radius-full, 999px);
        background: var(--brand-soft, #e6f9f3);
        width: var(--w, 0%);
        min-width: 2px;
      }
      .times {
        font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
      }
      .rule {
        display: block;
        padding-top: 12px;
        border-top: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
      }
      .entries li {
        display: grid;
        grid-template-columns: auto 1fr auto auto;
        align-items: baseline;
        gap: 10px;
        padding: 7px 0;
        border-bottom: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
      }
      .entries li:last-child {
        border-bottom: none;
      }
      .when {
        font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
        white-space: nowrap;
      }
      /* A feature name is arbitrary text from whichever app charged; it
         truncates rather than pushing the amount off the panel. */
      .what {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--text-strong, var(--fb-strong));
        font: var(--type-ui, 500 13px/1.3 "Geist", system-ui, sans-serif);
      }
      .amount {
        font-weight: var(--weight-medium, 500);
        color: var(--text-strong, var(--fb-strong));
      }
      /* Spending is the ordinary case here, so it stays in the body colour;
         only money arriving is worth a tone. Colouring every debit red would
         make a working account look like a page of errors. */
      .amount.credit {
        color: var(--success-fg, #0b8a68);
      }
      .after {
        font: var(--type-caption, 400 12px/1.35 "Geist", system-ui, sans-serif);
        min-width: 3.5em;
        text-align: right;
      }
      button.block {
        margin-top: 12px;
      }
    `],l([g({type:Number,attribute:"page-size"})],x.prototype,"pageSize",2),l([g({type:String,attribute:"app-id"})],x.prototype,"appId",2),l([g({type:Boolean,attribute:"no-summary"})],x.prototype,"noSummary",2),l([p()],x.prototype,"entries",2),l([p()],x.prototype,"cursor",2),l([p()],x.prototype,"complete",2),l([p()],x.prototype,"loaded",2),x=l([E("openapps-history")],x);function He(n){let e=n.app_name??n.app_id??null,t=n.ref_id??null;return e&&t?`${e} \xB7 ${t}`:e||t||"Spent"}function ft(n){switch(n.kind){case"debit":return He(n);case"topup":return"Credits purchased";case"referral_bonus":return"Referral bonus";case"adjustment":return n.ref_id?`Adjustment \u2014 ${n.ref_id}`:"Adjustment";case"refund":return n.amount<0?"Payment reversed":"Refund";default:return He(n)}}function Yt(n){let e=new Date(n*1e3);return Number.isNaN(e.getTime())?"":e.toLocaleDateString(void 0,{month:"short",day:"numeric"})}function mt(n){return new Date(n*1e3).toLocaleDateString(void 0,{day:"numeric",month:"short"})}var _=class extends m{constructor(){super(...arguments);this.appId="";this.info=null;this.earnings=null;this.referees=null;this.tab="link";this.copied=!1}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){let t=this.sdkOrNull;if(!t?.isLoggedIn){this.info=null,this.earnings=null,this.referees=null;return}this.info=await this.run(()=>t.referral.code(this.appId||void 0))??null,this.earnings=await this.run(()=>t.referral.earnings())??null,this.referees=await this.run(()=>t.referral.referees())??null}updated(t){t.has("appId")&&this.load()}get link(){if(this.info?.invite_url)return this.info.invite_url;let t=this.inviteUrl??(typeof location>"u"?"":`${location.origin}${location.pathname}`);if(!this.info)return t;let r=t.includes("?")?"&":"?";return`${t}${r}ref=${encodeURIComponent(this.info.code)}`}async copy(){try{await navigator.clipboard.writeText(this.link),this.copied=!0,setTimeout(()=>this.copied=!1,2e3)}catch{this.error="Could not copy. Select the link and copy it manually."}}render(){if(!this.sdkOrNull)return o`<p class="muted">Loading…</p>`;if(!this.sdk.isLoggedIn)return o`<p class="muted">Sign in to get your invite link.</p>`;if(!this.info)return o`<p class="muted">${this.error??"Loading\u2026"}</p>`;let t=this.referees?.referees??[],r=this.earnings?.entries??[],s=[["link","Your link"],["referees",`Referees${t.length?` (${t.length})`:""}`],["earnings",`Earnings${r.length?` (${r.length})`:""}`]];return o`
      <div class="stack">
        <div class="tabs" role="tablist">
          ${s.map(([i,a])=>o`
              <button
                role="tab"
                aria-selected=${this.tab===i}
                class="tab ${this.tab===i?"on":""}"
                @click=${()=>this.tab=i}
              >
                ${a}
              </button>
            `)}
        </div>
        ${this.tab==="link"?this.renderLink():this.tab==="referees"?this.renderReferees(t):this.renderEarnings(r)}
        ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
      </div>
    `}renderLink(){let t=this.earnings?.total??0,r=this.earnings?.entries.length??0;return o`
      <p class="desc">
        Share this link. When someone signs up through it and buys credits,
        you earn <strong>${this.info?.bonus_percent}%</strong> of what they
        buy, as credits.
      </p>
      <code class="payload">${this.link}</code>
      <div class="row">
        <button @click=${this.copy}>${this.copied?"Copied":"Copy link"}</button>
        <span class="muted mono">${this.info?.code}</span>
      </div>
      <div class="earned">
        <span class="eyebrow">Earned</span>
        <span class="total mono">${t.toLocaleString()}</span>
        <span class="caption">
          ${r===0?"No referred purchases yet.":`credits from ${r} purchase${r===1?"":"s"}`}
        </span>
      </div>
    `}renderReferees(t){return t.length===0?o`<p class="muted">
        Nobody has signed up through your link yet.
      </p>`:o`
      <p class="caption">
        Handles only — signing up through a link does not share someone's
        identity with you.
      </p>
      <div class="list">
        ${t.map(r=>o`
            <div class="item">
              <span class="mono handle">${r.handle}</span>
              <span class="grow caption">
                joined ${mt(r.joined_at)} ·
                ${r.purchases===0?"no purchases yet":`${r.purchases} purchase${r.purchases===1?"":"s"}`}
              </span>
              <span class="mono amount ${r.earned>0?"good":""}">
                ${r.earned>0?`+${r.earned.toLocaleString()}`:"\u2014"}
              </span>
            </div>
          `)}
      </div>
    `}renderEarnings(t){return t.length===0?o`<p class="muted">
        No referral earnings yet. A bonus is credited when a referee's
        purchase settles.
      </p>`:o`
      <p class="caption">
        Each row is one bonus, credited in the same transaction as the
        referee's purchase — so this list and your balance cannot disagree.
      </p>
      <div class="list">
        ${t.map(r=>o`
            <div class="item">
              <span class="mono date">${mt(r.created_at)}</span>
              <span class="grow caption">
                ${r.referee??"unknown"}
                ${r.referee_credits?o` bought ${r.referee_credits.toLocaleString()} credits`:c}
              </span>
              <span class="mono amount good">+${r.amount.toLocaleString()}</span>
            </div>
          `)}
      </div>
    `}};_.styles=[m.baseStyles,$`
      .stack {
        display: grid;
        gap: 12px;
      }
      .row {
        display: flex;
        align-items: center;
        gap: 10px;
      }
      .tabs {
        display: flex;
        gap: 4px;
        border-bottom: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
      }
      /* Underline tabs sitting on the hairline, with a 2px near-black
         indicator — not a filled pill, and never a coloured bar. The
         indicator is drawn on the tab itself and pulled down over the rule
         so the selected tab reads as continuous with its panel. */
      .tab {
        border: none;
        border-bottom: 2px solid transparent;
        border-radius: 0;
        background: transparent;
        color: var(--text-muted, var(--fb-muted));
        margin-bottom: -1px;
        padding: 0 10px;
      }
      .tab.on {
        border-bottom-color: var(--text-strong, var(--fb-strong));
        color: var(--text-strong, var(--fb-strong));
      }
      .tab:hover:not(.on) {
        color: var(--text-strong, var(--fb-strong));
        background: transparent;
      }
      .list {
        display: grid;
        border: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
        border-radius: var(--radius-lg, 12px);
        overflow: hidden;
      }
      .item {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 10px 14px;
      }
      .item + .item {
        border-top: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
      }
      .grow {
        flex: 1;
      }
      .date,
      .handle {
        font-size: var(--text-2xs, 11px);
        color: var(--text-faint, var(--fb-faint));
        min-width: 3.4em;
      }
      .amount {
        font-size: var(--text-sm, 13px);
        font-weight: var(--weight-medium, 500);
        color: var(--text-muted, var(--fb-muted));
      }
      .amount.good {
        color: var(--success-fg, #0b8a68);
      }
      .earned {
        display: grid;
        gap: 4px;
        padding: 14px 16px;
        border: var(--border-width, 1px) solid
          var(--border-hairline, var(--fb-hairline));
        border-radius: var(--radius-lg, 12px);
        background: var(--surface-card, var(--fb-card));
      }
      /* A balance is a quantity, so it is mono — same rule as the credit
         badge and the ledger. */
      .total {
        font-size: var(--text-3xl, 40px);
        font-weight: var(--weight-medium, 500);
        line-height: 1;
        letter-spacing: var(--tracking-display, -0.035em);
        color: var(--text-strong, var(--fb-strong));
      }
    `],l([g({type:String,attribute:"app-id"})],_.prototype,"appId",2),l([g({type:String,attribute:"invite-url"})],_.prototype,"inviteUrl",2),l([p()],_.prototype,"info",2),l([p()],_.prototype,"earnings",2),l([p()],_.prototype,"referees",2),l([p()],_.prototype,"tab",2),l([p()],_.prototype,"copied",2),_=l([E("openapps-referral")],_);var D,S=class extends m{constructor(){super(...arguments);this.rails="";this.returnTo="";this.packages=null;this.selected=null;this.instruction=null;this.topup=null;this.waiting=!1;ie(this,D)}connectedCallback(){super.connectedCallback(),this.load()}disconnectedCallback(){U(this,D)?.abort(),super.disconnectedCallback()}onSessionChange(){if(!this.packages){this.load();return}this.requestUpdate()}async load(){if(!this.sdkOrNull)return;let t=await this.run(()=>this.sdk.payments.packages());t&&(this.packages=t)}get offeredRails(){if(!this.packages)return[];let t=["stripe","ethereum","lightning"].filter(s=>this.packages?.rails?.[s]),r=this.rails.split(",").map(s=>s.trim()).filter(Boolean);return r.length?t.filter(s=>r.includes(s)):t}async start(t){let r=this.selected;r&&await this.run(async()=>{let s;switch(t){case"stripe":{let i=await this.sdk.payments.stripeCheckout(r.id,{returnTo:this.returnTo==="none"?null:this.returnTo||void 0});this.instruction={kind:"redirect"},!this.dispatchEvent(new CustomEvent("openapps-checkout",{detail:{url:i.checkout_url,packageId:r.id},cancelable:!0,bubbles:!0,composed:!0}))||(window.location.href=i.checkout_url);return}case"ethereum":{let i=await this.sdk.payments.ethDepositAddress(r.id);s=i.topup_id,this.instruction={kind:"address",chain:i.chain,address:i.address,amount:i.expected_amount};break}case"lightning":{let i=await this.sdk.payments.lightningInvoice(r.id);s=i.topup_id,this.instruction={kind:"invoice",bolt11:i.bolt11,amountMsat:i.amount_msat};break}}this.watch(s,Zt[t])})}async watch(t,r){U(this,D)?.abort();let s=new AbortController;ae(this,D,s),this.waiting=!0;try{let i=await this.sdk.payments.waitFor(t,{timeoutMs:r,signal:s.signal,onPoll:a=>{this.topup=a}});this.topup=i,i.status==="confirmed"&&(this.emit("openapps-topup",i),k())}catch(i){i instanceof Error&&i.name==="AbortError"||(this.error=rr(i))}finally{this.waiting=!1}}reset(){U(this,D)?.abort(),this.selected=null,this.instruction=null,this.topup=null,this.error=null}render(){return this.sdkOrNull?this.sdk.isLoggedIn?this.packages?this.instruction?this.renderInstruction(this.instruction):this.selected?this.renderRails(this.selected):this.renderPackages(this.packages.packages??[]):o`<p class="muted">${this.error??"Loading packages\u2026"}</p>`:o`<p class="muted">Sign in to buy credits.</p>`:o`<p class="muted">Loading…</p>`}renderPackages(t){return t.length===0?o`<p class="muted">No credit packages are configured.</p>`:o`
      <div class="grid">
        ${t.map(r=>o`
            <button class="package" @click=${()=>this.selected=r}>
              <span class="credits">
                ${r.credits.toLocaleString()} credits
              </span>
              <span class="perunit caption">${er(r)}</span>
              <span class="price">${gt(r.usd_price)}</span>
            </button>
          `)}
      </div>
      ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}renderRails(t){let r=this.offeredRails;return o`
      <p>
        <strong>${t.credits.toLocaleString()} credits</strong> —
        ${gt(t.usd_price)}
      </p>
      <div class="stack">
        ${r.map(s=>o`
            <button class="primary" ?disabled=${this.busy} @click=${()=>this.start(s)}>
              ${Qt[s]}
            </button>
          `)}
        ${r.length===0?o`<p class="muted">No payment methods are enabled.</p>`:c}
        <button @click=${this.reset}>Back</button>
      </div>
      ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}renderInstruction(t){let r=this.topup?.status??"pending";return r==="confirmed"?o`
        <span class="badge success"><span class="dot"></span>Confirmed</span>
        <p class="ok">Payment confirmed — credits added.</p>
        <button @click=${this.reset}>Buy more</button>
      `:r==="failed"||r==="expired"?o`
        <span class="badge danger"><span class="dot"></span>${r==="failed"?"Failed":"Expired"}</span>
        <p class="error" role="alert">This top-up ${r}. Nothing was charged.</p>
        <button @click=${this.reset}>Try again</button>
      `:o`
      ${t.kind==="redirect"?o`<p class="muted">Redirecting to checkout…</p>`:c}
      ${t.kind==="address"?o`
            <p>Send exactly <strong>${tr(t.amount,6)}</strong> USDC or
            USDT on <code>${t.chain}</code> to:</p>
            <code class="payload">${t.address}</code>
          `:c}
      ${t.kind==="invoice"?o`
            <p>Pay this Lightning invoice
            (<strong>${Math.ceil(t.amountMsat/1e3).toLocaleString()} sats</strong>):</p>
            <code class="payload">${t.bolt11}</code>
          `:c}
      ${t.kind!=="redirect"?o`
            <div class="row">
              <button @click=${()=>this.copy(Xt(t))}>Copy</button>
              <button @click=${this.reset}>Cancel</button>
            </div>
            ${this.renderWaiting()}
          `:c}
      ${this.error?o`<p class="error" role="alert">${this.error}</p>`:c}
    `}renderWaiting(){if(!this.waiting)return o`<p class="muted" aria-live="polite">Not watching for payment.</p>`;let t=this.topup?.confirmations;if(t===void 0)return o`<p class="muted" aria-live="polite">Waiting for payment…</p>`;let r=this.topup?.confirmations_required;if(r==null)return o`
        <p class="muted" aria-live="polite">
          Payment received — waiting for the network to finalise it.
        </p>
      `;let s=Math.min(t,r);return o`
      <p class="muted" aria-live="polite">
        Payment received — confirming (${s} of ${r}).
      </p>
      <progress
        class="confirms"
        max=${r}
        value=${s}
        aria-label="Confirmations"
      ></progress>
    `}async copy(t){try{await navigator.clipboard.writeText(t)}catch{this.error="Could not copy \u2014 select the text and copy it manually."}}};D=new WeakMap,S.styles=[m.baseStyles,$`
      /* Packs stack as rows rather than tiles: the numbers line up down the
         right edge, which is what makes three prices comparable at a glance. */
      .grid {
        display: grid;
        gap: 8px;
      }
      .package {
        display: flex;
        align-items: center;
        gap: 14px;
        min-height: auto;
        padding: 14px 16px;
        text-align: left;
        border-radius: var(--radius-lg, 12px);
      }
      .package:hover:not([disabled]) {
        border-color: var(--border-strong, var(--fb-hairline));
      }
      .credits {
        flex: 1;
        font: var(--weight-medium, 500) var(--text-base, 15px) / 1.2
          var(--font-sans, "Geist", system-ui, sans-serif);
        color: var(--text-strong, var(--fb-strong));
      }
      /* Money is always mono — balances, prices, per-unit rates, ledger
         amounts. Tabular figures keep the column aligned across packs. */
      .price {
        font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace);
        font-variant-numeric: tabular-nums;
        font-size: var(--text-md, 17px);
        color: var(--text-strong, var(--fb-strong));
      }
      .perunit {
        font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace);
        font-size: var(--text-2xs, 11px);
        color: var(--text-faint, var(--fb-faint));
      }
      .stack {
        display: flex;
        flex-direction: column;
        gap: 0.5em;
      }
      .row {
        display: flex;
        gap: 0.5em;
        margin-top: 0.5em;
      }
      .ok {
        font-weight: 600;
      }
      .confirms {
        display: block;
        width: 100%;
        height: 0.4em;
        margin-top: 0.35em;
        border: none;
        border-radius: 999px;
        overflow: hidden;
        background: var(--surface-card, var(--fb-card));
        accent-color: var(--brand, var(--fb-brand));
      }
      /* WebKit ignores accent-color on <progress>, so colour it directly. */
      .confirms::-webkit-progress-bar {
        background: var(--surface-card, var(--fb-card));
      }
      .confirms::-webkit-progress-value {
        background: var(--brand, var(--fb-brand));
      }
    `],l([g({type:String})],S.prototype,"rails",2),l([g({type:String,attribute:"return-to"})],S.prototype,"returnTo",2),l([p()],S.prototype,"packages",2),l([p()],S.prototype,"selected",2),l([p()],S.prototype,"instruction",2),l([p()],S.prototype,"topup",2),l([p()],S.prototype,"waiting",2),S=l([E("openapps-buy")],S);var Qt={stripe:"Pay by card",ethereum:"Pay with USDC / USDT",lightning:"Pay with Lightning"},Zt={stripe:void 0,lightning:void 0,ethereum:1800*1e3};function Xt(n){return n.kind==="address"?n.address:n.kind==="invoice"?n.bolt11:""}function gt(n){return`$${(n/100).toFixed(2)}`}function er(n){if(n.credits<=0)return"";let e=n.usd_price/n.credits;return`${e<1?e.toFixed(2):e.toFixed(1)}\xA2 each`}function tr(n,e){let t=10**e;return(n/t).toFixed(e).replace(/\.?0+$/,"")}function rr(n){let e=n instanceof Error?n.message:String(n);return e.includes("still pending")?"Still waiting on the network. Your credits will appear once the payment settles.":e}export{P as OpenAppsAccount,S as OpenAppsBuy,A as OpenAppsCredits,m as OpenAppsElement,x as OpenAppsHistory,w as OpenAppsLogin,_ as OpenAppsReferral,v as WalletError,Bt as availableNamespaces,ve as captureReferral,R as clearReferral,Be as configure,W as connectEthereum,Le as findNostrProvider,_t as getClient,k as notify,ke as onChange,oe as referralInUrl,B as signNostr,ze as signNostrWithBunker,Oe as signNostrWithSecretKey,j as signSiwe,ye as storedReferral,me as waitForNostrProvider};
//# sourceMappingURL=openapps-ui.js.map

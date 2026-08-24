const __vite__mapDeps=(i,m=__vite__mapDeps,d=(m.f||(m.f=["./C8r9ujnN.js","./X4SRYN_F.js","./CA4-7O7g.js","./wajNTN2x.js","./Cf8rvAt0.js"])))=>i.map(i=>d[i]);
var Ct=Object.defineProperty;var Ke=n=>{throw TypeError(n)};var St=(n,e,t)=>e in n?Ct(n,e,{enumerable:!0,configurable:!0,writable:!0,value:t}):n[e]=t;var S=(n,e,t)=>St(n,typeof e!="symbol"?e+"":e,t),Pe=(n,e,t)=>e.has(n)||Ke("Cannot "+t);var m=(n,e,t)=>(Pe(n,e,"read from private field"),t?t.call(n):e.get(n)),C=(n,e,t)=>e.has(n)?Ke("Cannot add the same private member more than once"):e instanceof WeakSet?e.add(n):e.set(n,t),$=(n,e,t,r)=>(Pe(n,e,"write to private field"),r?r.call(n,t):e.set(n,t),t),p=(n,e,t)=>(Pe(n,e,"access private method"),t);import{_ as Ie}from"./Dp1pzeXC.js";class k extends Error{constructor(t,r,s=0,i,a){super(r);S(this,"code");S(this,"status");S(this,"balance");S(this,"detail");this.name="OpenAppsError",this.code=t,this.status=s,this.balance=i,this.detail=a}get isAuthError(){return this.code==="unauthorized"}}const At={400:"bad_request",401:"unauthorized",402:"insufficient_balance",404:"not_found",409:"conflict",429:"rate_limited"};function Et(n,e){const t=e&&typeof e=="object"?e.error:void 0,r=t&&typeof t=="object"?t:void 0,s=(r==null?void 0:r.code)??At[n]??"internal",i=(r==null?void 0:r.message)??`request failed with status ${n}`;let a;if(s==="insufficient_balance"){const o=/-?\d+/.exec(i);o&&(a=Number(o[0]))}return new k(s,i,n,a,r)}function Me(n=null){let e=n;return{get:()=>e,set:t=>{e=t}}}function Ot(n="openapps.session"){let e=null;try{e=typeof localStorage<"u"?localStorage:null,e==null||e.setItem(n,e.getItem(n)??""),(e==null?void 0:e.getItem(n))===""&&e.removeItem(n)}catch{e=null}if(!e)return Me();const t=e;return{get(){const r=t.getItem(n);if(!r)return null;try{const s=JSON.parse(r);return s.accessToken&&s.refreshToken?s:null}catch{return null}},set(r){r?t.setItem(n,JSON.stringify(r)):t.removeItem(n)}}}function Pt(){try{return typeof localStorage<"u"?Ot():Me()}catch{return Me()}}const Nt=new Set(["confirmed","failed","expired"]);var j,A,ge,me,R,U,d,O,b,ke,ht,ut;class Tt{constructor(e){C(this,d);S(this,"baseUrl");C(this,j);C(this,A);C(this,ge);C(this,me);C(this,R,null);C(this,U,null);S(this,"auth",{methods:async e=>(await p(this,d,b).call(this,"/v1/auth/methods",{signal:e})).methods,challenge:(e,t,r)=>p(this,d,b).call(this,"/v1/auth/challenge",{method:"POST",body:{namespace:e,address:t},signal:r}),verify:async(e,t,r={})=>{const s=await p(this,d,b).call(this,"/v1/auth/verify",{method:"POST",body:{challenge_id:e,proof:t,referral_code:r.referralCode},signal:r.signal});return p(this,d,O).call(this,{accessToken:s.access_token,refreshToken:s.refresh_token}),s},googleStartUrl:(e,t)=>{const r=new URLSearchParams;e&&r.set("return_to",e),t&&r.set("ref",t);const s=r.toString();return`${this.baseUrl}/v1/auth/oidc/google/start${s?`?${s}`:""}`},completeRedirect:(e={})=>{const t=Ut(e,"code");return t?m(this,U)?m(this,U):($(this,U,(async()=>{try{const r=await p(this,d,b).call(this,"/v1/auth/oidc/exchange",{method:"POST",body:{code:t},signal:e.signal});return p(this,d,O).call(this,{accessToken:r.access_token,refreshToken:r.refresh_token}),e.hash===void 0&&e.url===void 0&&typeof history<"u"&&typeof location<"u"&&history.replaceState({},"",location.pathname+location.search),r}finally{$(this,U,null)}})()),m(this,U)):Promise.resolve(null)},me:e=>p(this,d,b).call(this,"/v1/me",{auth:"bearer",signal:e}),logout:async e=>{try{await p(this,d,b).call(this,"/v1/auth/logout",{method:"POST",auth:"bearer",signal:e})}finally{p(this,d,O).call(this,null)}},linkChallenge:(e,t,r)=>p(this,d,b).call(this,"/v1/auth/link/challenge",{method:"POST",auth:"bearer",body:{namespace:e,address:t},signal:r}),linkVerify:(e,t,r={})=>p(this,d,b).call(this,"/v1/auth/link/verify",{method:"POST",auth:"bearer",body:{challenge_id:e,proof:t,merge:r.merge??!1},signal:r.signal}),googleLinkStart:async(e,t={})=>(await p(this,d,b).call(this,"/v1/auth/link/oidc/google/start",{method:"POST",auth:"bearer",body:{return_to:e,merge:t.merge??!1},signal:t.signal})).auth_url,completeLinkRedirect:(e={})=>{const t=pt(e),r=t.get("linked"),s=t.get("link_conflict"),i=t.get("link_blocked"),a=t.get("link_error");if(!r&&!s&&!i&&!a)return null;if(e.hash===void 0&&e.url===void 0&&typeof history<"u"&&history.replaceState({},"",location.pathname+location.search),a)return{status:"error",message:a};if(i){const o=(t.get("clashes")??"").split(",").filter(Boolean),c=o.map(u=>({google:"Google",eip155:"wallet",nostr:"Nostr"})[u]??u).join(" and ");return{status:"blocked",namespaces:o,message:`That Google account belongs to another account which also has a ${c} sign-in, and so does this one. Disconnect it from the other account first.`}}return s?{status:"conflict",namespace:s,balance:Number(t.get("balance")??0)}:{status:"linked",namespace:r,merged:t.get("merged")==="1",credits:Number(t.get("credits")??0)}},unlink:(e,t)=>p(this,d,b).call(this,`/v1/auth/link/${encodeURIComponent(e)}`,{method:"DELETE",auth:"bearer",signal:t})});S(this,"credits",{balance:async e=>(await p(this,d,b).call(this,"/v1/credits/balance",{auth:"bearer",signal:e})).balance,deduct:(e,t,r,s)=>p(this,d,b).call(this,"/v1/credits/deduct",{method:"POST",auth:"app+bearer",body:{amount:e,reason:t,idempotency_key:r},signal:s}),history:(e={})=>p(this,d,b).call(this,"/v1/credits/history",{auth:"bearer",query:{cursor:e.cursor,limit:e.limit},signal:e.signal})});S(this,"payments",{packages:e=>p(this,d,b).call(this,"/v1/payments/packages",{signal:e}),stripeCheckout:(e,t={})=>p(this,d,b).call(this,"/v1/payments/stripe/checkout",{method:"POST",auth:"bearer",body:{package_id:e,return_to:t.returnTo===null?void 0:t.returnTo??Rt()},signal:t.signal}),ethDepositAddress:(e,t)=>p(this,d,b).call(this,"/v1/payments/eth/deposit-address",{method:"POST",auth:"bearer",body:{package_id:e},signal:t}),lightningInvoice:(e,t)=>p(this,d,b).call(this,"/v1/payments/lightning/invoice",{method:"POST",auth:"bearer",body:{package_id:e},signal:t}),list:e=>p(this,d,b).call(this,"/v1/payments/topups",{auth:"bearer",signal:e}),get:(e,t)=>p(this,d,b).call(this,`/v1/payments/topups/${encodeURIComponent(e)}`,{auth:"bearer",signal:t}),waitFor:async(e,t={})=>{var i,a;const r=t.intervalMs??2e3,s=Date.now()+(t.timeoutMs??900*1e3);for(;;){(i=t.signal)==null||i.throwIfAborted();try{const o=await this.payments.get(e,t.signal);if((a=t.onPoll)==null||a.call(t,o),Nt.has(o.status))return o}catch(o){if(o instanceof k&&o.code!=="network"||!(o instanceof k))throw o}if(Date.now()+r>s)throw new k("timeout",`top-up ${e} was still pending after the timeout`);await Lt(r,t.signal)}}});S(this,"referral",{code:e=>p(this,d,b).call(this,"/v1/referral/code",{auth:"bearer",signal:e}),apply:(e,t)=>p(this,d,b).call(this,"/v1/referral/apply",{method:"POST",auth:"bearer",body:{code:e},signal:t}),earnings:e=>p(this,d,b).call(this,"/v1/referral/earnings",{auth:"bearer",signal:e}),referees:e=>p(this,d,b).call(this,"/v1/referral/referees",{auth:"bearer",signal:e})});this.baseUrl=e.baseUrl.replace(/\/+$/,""),$(this,j,e.appKey),$(this,A,e.store??Pt());const t=e.fetch??globalThis.fetch;if(!t)throw new k("network","no fetch implementation available; pass one via options.fetch");$(this,ge,(r,s)=>t(r,s)),$(this,me,e.onAuthChange)}get session(){return m(this,A).get()}get isLoggedIn(){return m(this,A).get()!==null}adoptSession(e,t){p(this,d,O).call(this,{accessToken:e,refreshToken:t})}clearSession(){p(this,d,O).call(this,null)}}j=new WeakMap,A=new WeakMap,ge=new WeakMap,me=new WeakMap,R=new WeakMap,U=new WeakMap,d=new WeakSet,O=function(e){var t;m(this,A).set(e),(t=m(this,me))==null||t.call(this,e)},b=async function(e,t={}){const r=t.auth??"none";if(r!=="none"&&!m(this,A).get())throw new k("unauthorized","not logged in");if(r==="app+bearer"&&!m(this,j))throw new k("unauthorized","this call needs an app key; construct OpenApps with { appKey }");return p(this,d,ke).call(this,e,t,r,!0)},ke=async function(e,t,r,s){var u;let i=`${this.baseUrl}${e}`;if(t.query){const f=new URLSearchParams;for(const[_,x]of Object.entries(t.query))x!==void 0&&f.set(_,String(x));const g=f.toString();g&&(i+=`?${g}`)}const a={accept:"application/json"};t.body!==void 0&&(a["content-type"]="application/json"),r!=="none"&&(a.authorization=`Bearer ${((u=m(this,A).get())==null?void 0:u.accessToken)??""}`),r==="app+bearer"&&m(this,j)&&(a["x-openapps-app-key"]=m(this,j));let o;try{o=await m(this,ge).call(this,i,{method:t.method??"GET",headers:a,body:t.body===void 0?void 0:JSON.stringify(t.body),signal:t.signal})}catch(f){throw f instanceof Error&&f.name==="AbortError"?f:new k("network",f instanceof Error?f.message:"network request failed")}if(o.status===401&&r!=="none"&&s&&await p(this,d,ut).call(this))return p(this,d,ke).call(this,e,t,r,!1);const c=await p(this,d,ht).call(this,o);if(!o.ok){const f=Et(o.status,c);throw f.code==="unauthorized"&&r!=="none"&&p(this,d,O).call(this,null),f}return c},ht=async function(e){if(e.status===204)return null;const t=await e.text();if(!t)return null;try{return JSON.parse(t)}catch{throw new k(e.ok?"internal":"network",`expected JSON, got: ${t.slice(0,200)}`,e.status)}},ut=function(){if(m(this,R))return m(this,R);const e=m(this,A).get();return e?($(this,R,(async()=>{try{const t=await p(this,d,ke).call(this,"/v1/auth/refresh",{method:"POST",body:{refresh_token:e.refreshToken}},"none",!1),r={accessToken:t.access_token,refreshToken:t.refresh_token};return p(this,d,O).call(this,r),r}catch{return p(this,d,O).call(this,null),null}finally{$(this,R,null)}})()),m(this,R)):Promise.resolve(null)};function Rt(){if(!(typeof location>"u"))return`${location.origin}${location.pathname}${location.search}`}function pt(n){if(n.url!==void 0){const t=n.url,r=t.indexOf("#"),s=t.indexOf("?"),i=r>=0?t.slice(r+1):"",o=s>=0&&(r<0||s<r)?t.slice(s+1,r>=0?r:void 0):"",c=new URLSearchParams(i),u=new URLSearchParams(o);return{get:f=>c.get(f)??u.get(f)}}const e=n.hash??(typeof location>"u"?"":location.hash);return new URLSearchParams(e.replace(/^#/,""))}function Ut(n,e){return pt(n).get(e)}function Lt(n,e){return new Promise((t,r)=>{const s=setTimeout(()=>{e==null||e.removeEventListener("abort",i),t()},n),i=()=>{clearTimeout(s),r((e==null?void 0:e.reason)??new Error("aborted"))};e==null||e.addEventListener("abort",i,{once:!0})})}let de=null;function It(n){return de=new Tt(n),E(),de}function Or(){return de}function Mt(n,e){if(n)return n;if(de)return de;if(e)return It({baseUrl:e});throw new Error("no OpenApps client: call configure({ baseUrl }) or set base-url on the element")}const De=new Set;function Dt(n){return De.add(n),()=>De.delete(n)}function E(){for(const n of De)n()}/**
 * @license
 * Copyright 2019 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const _e=globalThis,He=_e.ShadowRoot&&(_e.ShadyCSS===void 0||_e.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,Ge=Symbol(),Ye=new WeakMap;let ft=class{constructor(e,t,r){if(this._$cssResult$=!0,r!==Ge)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=e,this.t=t}get styleSheet(){let e=this.o;const t=this.t;if(He&&e===void 0){const r=t!==void 0&&t.length===1;r&&(e=Ye.get(t)),e===void 0&&((this.o=e=new CSSStyleSheet).replaceSync(this.cssText),r&&Ye.set(t,e))}return e}toString(){return this.cssText}};const Wt=n=>new ft(typeof n=="string"?n:n+"",void 0,Ge),ne=(n,...e)=>{const t=n.length===1?n[0]:e.reduce((r,s,i)=>r+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(s)+n[i+1],n[0]);return new ft(t,n,Ge)},zt=(n,e)=>{if(He)n.adoptedStyleSheets=e.map(t=>t instanceof CSSStyleSheet?t:t.styleSheet);else for(const t of e){const r=document.createElement("style"),s=_e.litNonce;s!==void 0&&r.setAttribute("nonce",s),r.textContent=t.cssText,n.appendChild(r)}},Ze=He?n=>n:n=>n instanceof CSSStyleSheet?(e=>{let t="";for(const r of e.cssRules)t+=r.cssText;return Wt(t)})(n):n;/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const{is:jt,defineProperty:Ht,getOwnPropertyDescriptor:Gt,getOwnPropertyNames:Ft,getOwnPropertySymbols:Bt,getPrototypeOf:qt}=Object,L=globalThis,Qe=L.trustedTypes,Vt=Qe?Qe.emptyScript:"",Ne=L.reactiveElementPolyfillSupport,oe=(n,e)=>n,Ce={toAttribute(n,e){switch(e){case Boolean:n=n?Vt:null;break;case Object:case Array:n=n==null?n:JSON.stringify(n)}return n},fromAttribute(n,e){let t=n;switch(e){case Boolean:t=n!==null;break;case Number:t=n===null?null:Number(n);break;case Object:case Array:try{t=JSON.parse(n)}catch{t=null}}return t}},Fe=(n,e)=>!jt(n,e),Xe={attribute:!0,type:String,converter:Ce,reflect:!1,useDefault:!1,hasChanged:Fe};Symbol.metadata??(Symbol.metadata=Symbol("metadata")),L.litPropertyMetadata??(L.litPropertyMetadata=new WeakMap);let J=class extends HTMLElement{static addInitializer(e){this._$Ei(),(this.l??(this.l=[])).push(e)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(e,t=Xe){if(t.state&&(t.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(e)&&((t=Object.create(t)).wrapped=!0),this.elementProperties.set(e,t),!t.noAccessor){const r=Symbol(),s=this.getPropertyDescriptor(e,r,t);s!==void 0&&Ht(this.prototype,e,s)}}static getPropertyDescriptor(e,t,r){const{get:s,set:i}=Gt(this.prototype,e)??{get(){return this[t]},set(a){this[t]=a}};return{get:s,set(a){const o=s==null?void 0:s.call(this);i==null||i.call(this,a),this.requestUpdate(e,o,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(e){return this.elementProperties.get(e)??Xe}static _$Ei(){if(this.hasOwnProperty(oe("elementProperties")))return;const e=qt(this);e.finalize(),e.l!==void 0&&(this.l=[...e.l]),this.elementProperties=new Map(e.elementProperties)}static finalize(){if(this.hasOwnProperty(oe("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(oe("properties"))){const t=this.properties,r=[...Ft(t),...Bt(t)];for(const s of r)this.createProperty(s,t[s])}const e=this[Symbol.metadata];if(e!==null){const t=litPropertyMetadata.get(e);if(t!==void 0)for(const[r,s]of t)this.elementProperties.set(r,s)}this._$Eh=new Map;for(const[t,r]of this.elementProperties){const s=this._$Eu(t,r);s!==void 0&&this._$Eh.set(s,t)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(e){const t=[];if(Array.isArray(e)){const r=new Set(e.flat(1/0).reverse());for(const s of r)t.unshift(Ze(s))}else e!==void 0&&t.push(Ze(e));return t}static _$Eu(e,t){const r=t.attribute;return r===!1?void 0:typeof r=="string"?r:typeof e=="string"?e.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){var e;this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),(e=this.constructor.l)==null||e.forEach(t=>t(this))}addController(e){var t;(this._$EO??(this._$EO=new Set)).add(e),this.renderRoot!==void 0&&this.isConnected&&((t=e.hostConnected)==null||t.call(e))}removeController(e){var t;(t=this._$EO)==null||t.delete(e)}_$E_(){const e=new Map,t=this.constructor.elementProperties;for(const r of t.keys())this.hasOwnProperty(r)&&(e.set(r,this[r]),delete this[r]);e.size>0&&(this._$Ep=e)}createRenderRoot(){const e=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return zt(e,this.constructor.elementStyles),e}connectedCallback(){var e;this.renderRoot??(this.renderRoot=this.createRenderRoot()),this.enableUpdating(!0),(e=this._$EO)==null||e.forEach(t=>{var r;return(r=t.hostConnected)==null?void 0:r.call(t)})}enableUpdating(e){}disconnectedCallback(){var e;(e=this._$EO)==null||e.forEach(t=>{var r;return(r=t.hostDisconnected)==null?void 0:r.call(t)})}attributeChangedCallback(e,t,r){this._$AK(e,r)}_$ET(e,t){var i;const r=this.constructor.elementProperties.get(e),s=this.constructor._$Eu(e,r);if(s!==void 0&&r.reflect===!0){const a=(((i=r.converter)==null?void 0:i.toAttribute)!==void 0?r.converter:Ce).toAttribute(t,r.type);this._$Em=e,a==null?this.removeAttribute(s):this.setAttribute(s,a),this._$Em=null}}_$AK(e,t){var i,a;const r=this.constructor,s=r._$Eh.get(e);if(s!==void 0&&this._$Em!==s){const o=r.getPropertyOptions(s),c=typeof o.converter=="function"?{fromAttribute:o.converter}:((i=o.converter)==null?void 0:i.fromAttribute)!==void 0?o.converter:Ce;this._$Em=s;const u=c.fromAttribute(t,o.type);this[s]=u??((a=this._$Ej)==null?void 0:a.get(s))??u,this._$Em=null}}requestUpdate(e,t,r,s=!1,i){var a;if(e!==void 0){const o=this.constructor;if(s===!1&&(i=this[e]),r??(r=o.getPropertyOptions(e)),!((r.hasChanged??Fe)(i,t)||r.useDefault&&r.reflect&&i===((a=this._$Ej)==null?void 0:a.get(e))&&!this.hasAttribute(o._$Eu(e,r))))return;this.C(e,t,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(e,t,{useDefault:r,reflect:s,wrapped:i},a){r&&!(this._$Ej??(this._$Ej=new Map)).has(e)&&(this._$Ej.set(e,a??t??this[e]),i!==!0||a!==void 0)||(this._$AL.has(e)||(this.hasUpdated||r||(t=void 0),this._$AL.set(e,t)),s===!0&&this._$Em!==e&&(this._$Eq??(this._$Eq=new Set)).add(e))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(t){Promise.reject(t)}const e=this.scheduleUpdate();return e!=null&&await e,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){var r;if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??(this.renderRoot=this.createRenderRoot()),this._$Ep){for(const[i,a]of this._$Ep)this[i]=a;this._$Ep=void 0}const s=this.constructor.elementProperties;if(s.size>0)for(const[i,a]of s){const{wrapped:o}=a,c=this[i];o!==!0||this._$AL.has(i)||c===void 0||this.C(i,void 0,a,c)}}let e=!1;const t=this._$AL;try{e=this.shouldUpdate(t),e?(this.willUpdate(t),(r=this._$EO)==null||r.forEach(s=>{var i;return(i=s.hostUpdate)==null?void 0:i.call(s)}),this.update(t)):this._$EM()}catch(s){throw e=!1,this._$EM(),s}e&&this._$AE(t)}willUpdate(e){}_$AE(e){var t;(t=this._$EO)==null||t.forEach(r=>{var s;return(s=r.hostUpdated)==null?void 0:s.call(r)}),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(e)),this.updated(e)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(e){return!0}update(e){this._$Eq&&(this._$Eq=this._$Eq.forEach(t=>this._$ET(t,this[t]))),this._$EM()}updated(e){}firstUpdated(e){}};J.elementStyles=[],J.shadowRootOptions={mode:"open"},J[oe("elementProperties")]=new Map,J[oe("finalized")]=new Map,Ne==null||Ne({ReactiveElement:J}),(L.reactiveElementVersions??(L.reactiveElementVersions=[])).push("2.1.2");/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const le=globalThis,et=n=>n,Se=le.trustedTypes,tt=Se?Se.createPolicy("lit-html",{createHTML:n=>n}):void 0,gt="$lit$",T=`lit$${Math.random().toFixed(9).slice(2)}$`,mt="?"+T,Jt=`<${mt}>`,F=document,he=()=>F.createComment(""),ue=n=>n===null||typeof n!="object"&&typeof n!="function",Be=Array.isArray,Kt=n=>Be(n)||typeof(n==null?void 0:n[Symbol.iterator])=="function",Te=`[ 	
\f\r]`,ie=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,rt=/-->/g,nt=/>/g,W=RegExp(`>|${Te}(?:([^\\s"'>=/]+)(${Te}*=${Te}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),st=/'/g,it=/"/g,bt=/^(?:script|style|textarea|title)$/i,vt=n=>(e,...t)=>({_$litType$:n,strings:e,values:t}),l=vt(1),qe=vt(2),te=Symbol.for("lit-noChange"),h=Symbol.for("lit-nothing"),at=new WeakMap,z=F.createTreeWalker(F,129);function yt(n,e){if(!Be(n)||!n.hasOwnProperty("raw"))throw Error("invalid template strings array");return tt!==void 0?tt.createHTML(e):e}const Yt=(n,e)=>{const t=n.length-1,r=[];let s,i=e===2?"<svg>":e===3?"<math>":"",a=ie;for(let o=0;o<t;o++){const c=n[o];let u,f,g=-1,_=0;for(;_<c.length&&(a.lastIndex=_,f=a.exec(c),f!==null);)_=a.lastIndex,a===ie?f[1]==="!--"?a=rt:f[1]!==void 0?a=nt:f[2]!==void 0?(bt.test(f[2])&&(s=RegExp("</"+f[2],"g")),a=W):f[3]!==void 0&&(a=W):a===W?f[0]===">"?(a=s??ie,g=-1):f[1]===void 0?g=-2:(g=a.lastIndex-f[2].length,u=f[1],a=f[3]===void 0?W:f[3]==='"'?it:st):a===it||a===st?a=W:a===rt||a===nt?a=ie:(a=W,s=void 0);const x=a===W&&n[o+1].startsWith("/>")?" ":"";i+=a===ie?c+Jt:g>=0?(r.push(u),c.slice(0,g)+gt+c.slice(g)+T+x):c+T+(g===-2?o:x)}return[yt(n,i+(n[t]||"<?>")+(e===2?"</svg>":e===3?"</math>":"")),r]};class pe{constructor({strings:e,_$litType$:t},r){let s;this.parts=[];let i=0,a=0;const o=e.length-1,c=this.parts,[u,f]=Yt(e,t);if(this.el=pe.createElement(u,r),z.currentNode=this.el.content,t===2||t===3){const g=this.el.content.firstChild;g.replaceWith(...g.childNodes)}for(;(s=z.nextNode())!==null&&c.length<o;){if(s.nodeType===1){if(s.hasAttributes())for(const g of s.getAttributeNames())if(g.endsWith(gt)){const _=f[a++],x=s.getAttribute(g).split(T),$e=/([.?@])?(.*)/.exec(_);c.push({type:1,index:i,name:$e[2],strings:x,ctor:$e[1]==="."?Qt:$e[1]==="?"?Xt:$e[1]==="@"?er:Ae}),s.removeAttribute(g)}else g.startsWith(T)&&(c.push({type:6,index:i}),s.removeAttribute(g));if(bt.test(s.tagName)){const g=s.textContent.split(T),_=g.length-1;if(_>0){s.textContent=Se?Se.emptyScript:"";for(let x=0;x<_;x++)s.append(g[x],he()),z.nextNode(),c.push({type:2,index:++i});s.append(g[_],he())}}}else if(s.nodeType===8)if(s.data===mt)c.push({type:2,index:i});else{let g=-1;for(;(g=s.data.indexOf(T,g+1))!==-1;)c.push({type:7,index:i}),g+=T.length-1}i++}}static createElement(e,t){const r=F.createElement("template");return r.innerHTML=e,r}}function re(n,e,t=n,r){var a,o;if(e===te)return e;let s=r!==void 0?(a=t._$Co)==null?void 0:a[r]:t._$Cl;const i=ue(e)?void 0:e._$litDirective$;return(s==null?void 0:s.constructor)!==i&&((o=s==null?void 0:s._$AO)==null||o.call(s,!1),i===void 0?s=void 0:(s=new i(n),s._$AT(n,t,r)),r!==void 0?(t._$Co??(t._$Co=[]))[r]=s:t._$Cl=s),s!==void 0&&(e=re(n,s._$AS(n,e.values),s,r)),e}class Zt{constructor(e,t){this._$AV=[],this._$AN=void 0,this._$AD=e,this._$AM=t}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(e){const{el:{content:t},parts:r}=this._$AD,s=((e==null?void 0:e.creationScope)??F).importNode(t,!0);z.currentNode=s;let i=z.nextNode(),a=0,o=0,c=r[0];for(;c!==void 0;){if(a===c.index){let u;c.type===2?u=new ve(i,i.nextSibling,this,e):c.type===1?u=new c.ctor(i,c.name,c.strings,this,e):c.type===6&&(u=new tr(i,this,e)),this._$AV.push(u),c=r[++o]}a!==(c==null?void 0:c.index)&&(i=z.nextNode(),a++)}return z.currentNode=F,s}p(e){let t=0;for(const r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(e,r,t),t+=r.strings.length-2):r._$AI(e[t])),t++}}class ve{get _$AU(){var e;return((e=this._$AM)==null?void 0:e._$AU)??this._$Cv}constructor(e,t,r,s){this.type=2,this._$AH=h,this._$AN=void 0,this._$AA=e,this._$AB=t,this._$AM=r,this.options=s,this._$Cv=(s==null?void 0:s.isConnected)??!0}get parentNode(){let e=this._$AA.parentNode;const t=this._$AM;return t!==void 0&&(e==null?void 0:e.nodeType)===11&&(e=t.parentNode),e}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(e,t=this){e=re(this,e,t),ue(e)?e===h||e==null||e===""?(this._$AH!==h&&this._$AR(),this._$AH=h):e!==this._$AH&&e!==te&&this._(e):e._$litType$!==void 0?this.$(e):e.nodeType!==void 0?this.T(e):Kt(e)?this.k(e):this._(e)}O(e){return this._$AA.parentNode.insertBefore(e,this._$AB)}T(e){this._$AH!==e&&(this._$AR(),this._$AH=this.O(e))}_(e){this._$AH!==h&&ue(this._$AH)?this._$AA.nextSibling.data=e:this.T(F.createTextNode(e)),this._$AH=e}$(e){var i;const{values:t,_$litType$:r}=e,s=typeof r=="number"?this._$AC(e):(r.el===void 0&&(r.el=pe.createElement(yt(r.h,r.h[0]),this.options)),r);if(((i=this._$AH)==null?void 0:i._$AD)===s)this._$AH.p(t);else{const a=new Zt(s,this),o=a.u(this.options);a.p(t),this.T(o),this._$AH=a}}_$AC(e){let t=at.get(e.strings);return t===void 0&&at.set(e.strings,t=new pe(e)),t}k(e){Be(this._$AH)||(this._$AH=[],this._$AR());const t=this._$AH;let r,s=0;for(const i of e)s===t.length?t.push(r=new ve(this.O(he()),this.O(he()),this,this.options)):r=t[s],r._$AI(i),s++;s<t.length&&(this._$AR(r&&r._$AB.nextSibling,s),t.length=s)}_$AR(e=this._$AA.nextSibling,t){var r;for((r=this._$AP)==null?void 0:r.call(this,!1,!0,t);e!==this._$AB;){const s=et(e).nextSibling;et(e).remove(),e=s}}setConnected(e){var t;this._$AM===void 0&&(this._$Cv=e,(t=this._$AP)==null||t.call(this,e))}}class Ae{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(e,t,r,s,i){this.type=1,this._$AH=h,this._$AN=void 0,this.element=e,this.name=t,this._$AM=s,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=h}_$AI(e,t=this,r,s){const i=this.strings;let a=!1;if(i===void 0)e=re(this,e,t,0),a=!ue(e)||e!==this._$AH&&e!==te,a&&(this._$AH=e);else{const o=e;let c,u;for(e=i[0],c=0;c<i.length-1;c++)u=re(this,o[r+c],t,c),u===te&&(u=this._$AH[c]),a||(a=!ue(u)||u!==this._$AH[c]),u===h?e=h:e!==h&&(e+=(u??"")+i[c+1]),this._$AH[c]=u}a&&!s&&this.j(e)}j(e){e===h?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,e??"")}}class Qt extends Ae{constructor(){super(...arguments),this.type=3}j(e){this.element[this.name]=e===h?void 0:e}}class Xt extends Ae{constructor(){super(...arguments),this.type=4}j(e){this.element.toggleAttribute(this.name,!!e&&e!==h)}}class er extends Ae{constructor(e,t,r,s,i){super(e,t,r,s,i),this.type=5}_$AI(e,t=this){if((e=re(this,e,t,0)??h)===te)return;const r=this._$AH,s=e===h&&r!==h||e.capture!==r.capture||e.once!==r.once||e.passive!==r.passive,i=e!==h&&(r===h||s);s&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,e),this._$AH=e}handleEvent(e){var t;typeof this._$AH=="function"?this._$AH.call(((t=this.options)==null?void 0:t.host)??this.element,e):this._$AH.handleEvent(e)}}class tr{constructor(e,t,r){this.element=e,this.type=6,this._$AN=void 0,this._$AM=t,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(e){re(this,e)}}const Re=le.litHtmlPolyfillSupport;Re==null||Re(pe,ve),(le.litHtmlVersions??(le.litHtmlVersions=[])).push("3.3.3");const rr=(n,e,t)=>{const r=(t==null?void 0:t.renderBefore)??e;let s=r._$litPart$;if(s===void 0){const i=(t==null?void 0:t.renderBefore)??null;r._$litPart$=s=new ve(e.insertBefore(he(),i),i,void 0,t??{})}return s._$AI(n),s};/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const G=globalThis;class ce extends J{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){var t;const e=super.createRenderRoot();return(t=this.renderOptions).renderBefore??(t.renderBefore=e.firstChild),e}update(e){const t=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(e),this._$Do=rr(t,this.renderRoot,this.renderOptions)}connectedCallback(){var e;super.connectedCallback(),(e=this._$Do)==null||e.setConnected(!0)}disconnectedCallback(){var e;super.disconnectedCallback(),(e=this._$Do)==null||e.setConnected(!1)}render(){return te}}var dt;ce._$litElement$=!0,ce.finalized=!0,(dt=G.litElementHydrateSupport)==null||dt.call(G,{LitElement:ce});const Ue=G.litElementPolyfillSupport;Ue==null||Ue({LitElement:ce});(G.litElementVersions??(G.litElementVersions=[])).push("4.2.2");/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const ye=n=>(e,t)=>{t!==void 0?t.addInitializer(()=>{customElements.define(n,e)}):customElements.define(n,e)};/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */const nr={attribute:!0,type:String,converter:Ce,reflect:!1,hasChanged:Fe},sr=(n=nr,e,t)=>{const{kind:r,metadata:s}=t;let i=globalThis.litPropertyMetadata.get(s);if(i===void 0&&globalThis.litPropertyMetadata.set(s,i=new Map),r==="setter"&&((n=Object.create(n)).wrapped=!0),i.set(t.name,n),r==="accessor"){const{name:a}=t;return{set(o){const c=e.get.call(this);e.set.call(this,o),this.requestUpdate(a,c,n,!0,o)},init(o){return o!==void 0&&this.C(a,void 0,n,o),o}}}if(r==="setter"){const{name:a}=t;return function(o){const c=this[a];e.call(this,o),this.requestUpdate(a,c,n,!0,o)}}throw Error("Unsupported decorator location: "+r)};function N(n){return(e,t)=>typeof t=="object"?sr(n,e,t):((r,s,i)=>{const a=s.hasOwnProperty(i);return s.constructor.createProperty(i,r),a?Object.getOwnPropertyDescriptor(s,i):void 0})(n,e,t)}/**
 * @license
 * Copyright 2017 Google LLC
 * SPDX-License-Identifier: BSD-3-Clause
 */function v(n){return N({...n,state:!0,attribute:!1})}var Ee=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i},be;const Je=class Je extends ce{constructor(){super(...arguments);C(this,be);this.error=null,this.busy=!1}connectedCallback(){super.connectedCallback(),$(this,be,Dt(()=>this.onSessionChange()))}disconnectedCallback(){var t;(t=m(this,be))==null||t.call(this),super.disconnectedCallback()}onSessionChange(){this.requestUpdate()}get sdk(){return Mt(this.client,this.baseUrl)}get sdkOrNull(){try{return this.sdk}catch{return null}}async run(t){this.error=null,this.busy=!0;try{return await t()}catch(r){if(r instanceof Error&&r.name==="AbortError")return;this.error=ir(r);return}finally{this.busy=!1}}emit(t,r){this.dispatchEvent(new CustomEvent(t,{detail:r,bubbles:!0,composed:!0}))}};be=new WeakMap,Je.baseStyles=ne`
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
  `;let w=Je;Ee([N({type:String,attribute:"base-url"})],w.prototype,"baseUrl",void 0);Ee([N({attribute:!1})],w.prototype,"client",void 0);Ee([v()],w.prototype,"error",void 0);Ee([v()],w.prototype,"busy",void 0);function ir(n){if(n instanceof k)switch(n.code){case"unauthorized":return"Please sign in again.";case"insufficient_balance":return n.balance===void 0?"Not enough credits.":`Not enough credits — you have ${n.balance}.`;case"rate_limited":return"Too many attempts. Please wait a moment and try again.";case"network":return"Could not reach the server. Check your connection.";case"timeout":return"This is taking longer than expected. It may still complete.";default:return n.message}return n instanceof Error?n.message:String(n)}const ar=qe`<svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true" focusable="false"><path fill="#4285F4" d="M17.64 9.205c0-.639-.057-1.252-.164-1.841H9v3.481h4.844a4.14 4.14 0 0 1-1.796 2.716v2.258h2.909c1.702-1.567 2.683-3.874 2.683-6.614z"/><path fill="#34A853" d="M9 18c2.43 0 4.467-.806 5.956-2.181l-2.909-2.258c-.806.54-1.837.859-3.047.859-2.344 0-4.328-1.583-5.036-3.71H.957v2.332A8.997 8.997 0 0 0 9 18z"/><path fill="#FBBC05" d="M3.964 10.71A5.41 5.41 0 0 1 3.682 9c0-.593.102-1.17.282-1.71V4.958H.957A8.996 8.996 0 0 0 0 9c0 1.452.348 2.827.957 4.042l3.007-2.332z"/><path fill="#EA4335" d="M9 3.58c1.321 0 2.508.454 3.44 1.346l2.582-2.582C13.463.892 11.426 0 9 0A8.997 8.997 0 0 0 .957 4.958L3.964 7.29C4.672 5.163 6.656 3.58 9 3.58z"/></svg>`,or=qe`<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false"><g fill="#627EEA"><path d="M12 1.5 5.75 12.02 12 15.73V1.5z" opacity=".55"/><path d="M12 1.5v14.23l6.25-3.71L12 1.5z" opacity=".85"/><path d="M12 17.06 5.75 13.35 12 22.5v-5.44z" opacity=".55"/><path d="M12 22.5v-5.44l6.25-3.71L12 22.5z" opacity=".85"/></g></svg>`,lr=qe`<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false"><path fill="#9C59FF" d="M3 23.9C2.7 23.6 2.8 22.7 3.3 22.1C3.4 21.9 3.4 21.9 3.2 21.9C3 22 2.9 21.9 2.9 21.6C3 21.3 3.4 21 3.9 20.9C4.4 20.8 4.4 20.8 5.2 19.5C5.7 18.9 6.3 18.1 6.5 17.7C6.9 17.2 7 17 7.1 16.8C7.2 16.3 7.4 16 7.9 15.8C8.5 15.6 10.4 13.8 10.2 13.6C10.2 13.6 10 13.6 9.7 13.5C8.7 13.3 7.5 12.8 6.9 12.4C6.6 12.2 6.6 12.2 6.4 12.2C5.6 12.3 4.9 12.5 4.4 12.8C3.8 13.1 3.8 13.2 3.7 13.1C3.6 13 3.6 12.4 3.8 12C3.8 11.9 3.8 11.9 3.7 11.9C3.6 11.9 3.4 12 3.1 12.3C2.7 12.8 2.6 12.8 2.5 12.6C2.3 12.4 2.4 12.1 2.5 11.7C2.6 11.2 2.6 11.2 2.5 11.2C2.4 11.3 2.2 11.4 2 11.4C1.6 11.6 1.5 11.6 1.5 11.4C1.5 10.7 2.3 9.7 3 9.3C3.7 8.9 4.9 8.8 5.2 9C5.3 9.1 5.6 9.2 5.6 9.2C5.6 9.2 5.6 9.1 5.5 9C5.4 8.8 5.4 8.6 5.5 8.6C5.5 8.6 5.9 8.6 6.3 8.6C7.6 8.6 8.1 8.4 9.4 7.8C10.9 7.1 11.1 7 11.7 6.8C12.5 6.5 12.9 6.4 13.9 6.4C15.4 6.3 16 6.5 17.4 7.3C18 7.7 18.1 7.7 18.4 7.6C18.7 7.6 18.8 7.6 19.1 7.6C19.7 7.7 20 7.7 20.4 7.5C21.1 7.1 21.4 6.5 21.3 5.7C21.3 5 21.1 4.7 20.2 4.1C19.1 3.2 18.7 2.5 18.7 1.5C18.7 0.9 18.8 0.6 19.1 0.3C19.5 -0.1 19.9 -0.1 20.6 0.4C21 0.6 21.2 0.7 21.8 0.9C22.6 1.2 22.7 1.2 22.2 1.3C21.8 1.3 21.8 1.3 22.1 1.4C22.7 1.6 22.6 1.7 21.7 1.7C21.1 1.7 20.9 1.7 20.6 1.8C20.1 1.9 20 2 20.1 2.2C20.1 2.5 20.2 2.6 20.9 3.1C22.1 4.1 22.5 4.8 22.5 6C22.4 7.5 21.5 8.7 19.8 9.8C19.2 10.1 19.2 10.1 19.2 10.7C19.2 11.9 19 12.5 18.3 13.1C17.5 13.7 16.6 13.9 15.1 14L14.3 14L14.2 14.2C14.1 14.2 14.1 14.3 14.1 14.4C14.1 14.4 13.8 14.6 13.5 14.8C13.2 15 12.6 15.8 12.9 15.7C12.9 15.7 13.4 15.5 14 15.3C17 14.4 16.7 14.5 17.2 14.5C17.8 14.5 17.8 14.5 18.4 15.4C19 16.3 19.1 16.5 19 16.6C19 16.8 18.5 16.6 18 16.1C17.7 15.8 17.6 15.8 17.7 16.1C17.7 16.4 17.6 16.5 17.4 16.4C17.3 16.3 17.2 16.2 17.1 15.8L17.1 15.5L16.9 15.5C16.6 15.5 16.6 15.5 14.5 16.2C13.3 16.6 12.9 16.7 12.7 16.9C12 17.2 11.5 17 11.5 16.3C11.5 16.1 11.9 14.9 12.1 14.8C12.1 14.8 12.3 14.4 12.2 14.4C12.2 14.4 11.9 14.5 11.6 14.6L11 14.8L10 15.6C9 16.4 9 16.4 8.9 16.6C8.8 17 8.5 17.3 8.1 17.4C7.9 17.5 7.8 17.7 6.9 18.8C5.9 20.1 5.3 20.9 4.9 21.6C4.8 21.8 4.5 22.1 4.3 22.4C3.7 22.9 3.6 23.1 3.3 23.6C3.1 24 3.1 24 3 23.9Z"/></svg>`;class y extends Error{constructor(e){super(e),this.name="WalletError"}}function wt(){var n;return typeof window>"u"?[]:[{where:"window.nostr",provider:window.nostr},{where:"window.okxwallet.nostr",provider:(n=window.okxwallet)==null?void 0:n.nostr}]}function cr(n){const e=n;return!!e&&typeof e.getPublicKey=="function"&&typeof e.signEvent=="function"}function dr(){for(const{provider:n}of wt())if(cr(n))return n;return null}async function $t(n=2e3){const e=Date.now()+n;for(;;){const t=dr();if(t)return t;const r=e-Date.now();if(r<=0)return null;await new Promise(s=>setTimeout(s,Math.min(100,r)))}}function kt(){return wt().map(n=>n.where)}function _t(){if(typeof window>"u")return null;for(const n of[window.ethereum,window.okxwallet])if(n&&typeof n.request=="function")return n;return null}async function hr(n,e){let t;try{t=JSON.parse(n)}catch{throw new y("server sent an unreadable Nostr challenge")}const{nip19:r,finalizeEvent:s}=await Ie(async()=>{const{nip19:a,finalizeEvent:o}=await import("./C8r9ujnN.js");return{nip19:a,finalizeEvent:o}},__vite__mapDeps([0,1,2]),import.meta.url);let i;try{const a=r.decode(e.trim());if(a.type!=="nsec")throw new y(`that is an ${a.type} key — sign-in needs the secret key, which starts with nsec1`);i=a.data}catch(a){throw a instanceof y?a:new y("that does not look like a valid nsec1… key")}try{const a=s({kind:t.kind,content:t.content,tags:t.tags,created_at:t.created_at??Math.floor(Date.now()/1e3)},i);return{type:"nostr_event",event:JSON.stringify(a)}}finally{i.fill(0)}}async function We(){const n=_t();if(!n)throw new y("no Ethereum wallet found in this browser");let e;try{e=await n.request({method:"eth_requestAccounts"})}catch(r){throw new y(Ve(r,"wallet connection was rejected"))}const t=Array.isArray(e)?e[0]:void 0;if(typeof t!="string"||!t)throw new y("wallet returned no accounts");return t}async function ze(n,e){const t=_t();if(!t)throw new y("no Ethereum wallet found in this browser");try{const r=await t.request({method:"personal_sign",params:[n,e]});if(typeof r!="string")throw new y("wallet returned no signature");return{type:"signature",signature:r}}catch(r){throw r instanceof y?r:new y(Ve(r,"signature was rejected"))}}async function je(n){const e=await $t();if(!e)throw new y(`no Nostr signer answered (looked at ${kt().join(", ")})`);let t;try{t=JSON.parse(n)}catch{throw new y("server sent an unreadable Nostr challenge")}t.created_at??(t.created_at=Math.floor(Date.now()/1e3));try{const r=await e.signEvent(t);return{type:"nostr_event",event:JSON.stringify(r)}}catch(r){throw new y(Ve(r,"signing was rejected"))}}const ur=6e4;async function pr(n,e,t={}){let r;try{r=JSON.parse(n)}catch{throw new y("server sent an unreadable Nostr challenge")}const[{BunkerSigner:s,parseBunkerInput:i},{generateSecretKey:a}]=await Promise.all([Ie(()=>import("./wajNTN2x.js"),__vite__mapDeps([3,1,2]),import.meta.url),Ie(()=>import("./Cf8rvAt0.js"),__vite__mapDeps([4,1]),import.meta.url)]),o=await i(e.trim()).catch(()=>null);if(!o)throw new y("that is not a bunker:// address or a NIP-05 name — copy the connection string from your signer app");const c=s.fromBunker(a(),o,{onauth:u=>{var f;return(f=t.onAuthUrl)==null?void 0:f.call(t,u)}});try{const u=await fr((async()=>(await c.connect(),c.signEvent({kind:r.kind,content:r.content,tags:r.tags,created_at:r.created_at??Math.floor(Date.now()/1e3)})))(),t.timeoutMs??ur,"the signer did not respond — check it is running and try again");return{type:"nostr_event",event:JSON.stringify(u)}}catch(u){throw u instanceof y?u:new y(u instanceof Error?u.message:"the remote signer refused")}finally{await c.close().catch(()=>{})}}function fr(n,e,t){return new Promise((r,s)=>{const i=setTimeout(()=>s(new y(t)),e);n.then(a=>{clearTimeout(i),r(a)},a=>{clearTimeout(i),s(a)})})}function Ve(n,e){if(n&&typeof n=="object"){const t=n;if(t.code===4001)return e;if(t.message)return t.message}return e}var D=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i},K;let P=(K=class extends w{constructor(){super(...arguments),this.me=null,this.enabled=null,this.signerTimeout=2e3,this.variant="inline",this.nostrFallback="none",this.nostrHint=null,this.authUrl=null}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){const e=await this.run(()=>this.sdk.auth.completeRedirect());if(e&&(this.emit("openapps-login",e),E()),this.enabled||(this.enabled=await this.run(()=>this.sdk.auth.methods())??null),!this.sdk.isLoggedIn){this.me=null;return}this.me=await this.run(()=>this.sdk.auth.me())??null}async loginWithWallet(){await this.run(async()=>{const e=await We(),t=await this.sdk.auth.challenge("eip155",e),r=await ze(t.message,e),s=await this.sdk.auth.verify(t.challenge_id,r,{referralCode:ae()});this.emit("openapps-login",s),E()})}async loginWithNostr(){if(!await $t(this.signerTimeout)){this.nostrFallback="bunker",this.nostrHint=`No signer extension answered. Checked ${kt().join(" and ")}. On a phone, or without an extension, connect a remote signer below.`;return}await this.run(async()=>{const e=await this.sdk.auth.challenge("nostr"),t=await je(e.message),r=await this.sdk.auth.verify(e.challenge_id,t,{referralCode:ae()});this.emit("openapps-login",r),E()})}async loginWithBunker(e){e.preventDefault();const t=this.renderRoot.querySelector("#bunker"),r=(t==null?void 0:t.value.trim())??"";r&&(this.authUrl=null,await this.run(async()=>{const s=await this.sdk.auth.challenge("nostr"),i=await pr(s.message,r,{onAuthUrl:o=>{this.authUrl=o}}),a=await this.sdk.auth.verify(s.challenge_id,i,{referralCode:ae()});this.nostrFallback="none",this.authUrl=null,this.emit("openapps-login",a),E()}))}async loginWithNsec(e){e.preventDefault();const t=this.renderRoot.querySelector("#nsec"),r=(t==null?void 0:t.value.trim())??"";r&&await this.run(async()=>{try{const s=await this.sdk.auth.challenge("nostr"),i=await hr(s.message,r),a=await this.sdk.auth.verify(s.challenge_id,i,{referralCode:ae()});this.nostrFallback="none",this.emit("openapps-login",a),E()}finally{t&&(t.value="")}})}loginWithGoogle(){const e=`${location.origin}${location.pathname}${location.search}`;window.location.href=this.sdk.auth.googleStartUrl(e,ae())}async logout(){await this.run(()=>this.sdk.auth.logout()),this.me=null,this.emit("openapps-logout",null),E()}render(){var i,a,o;if(this.me)return this.renderSignedIn(this.me);const e=((i=this.enabled)==null?void 0:i.google)??!1,t=((a=this.enabled)==null?void 0:a.eip155)??!1,r=((o=this.enabled)==null?void 0:o.nostr)??!1;if(this.enabled&&!e&&!t&&!r)return this.frame(l`
        <p class="muted">This server has no login methods configured.</p>
        ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
      `);const s=this.variant==="panel"?"block":"";return this.frame(l`
      <div class="stack">
        ${e?l`<button
              class="provider ${s}"
              ?disabled=${this.busy}
              @click=${this.loginWithGoogle}
            >
              ${ar}<span>Continue with Google</span>
            </button>`:h}
        ${t?l`<button
              class="provider ${s}"
              ?disabled=${this.busy}
              @click=${this.loginWithWallet}
            >
              ${or}<span>Continue with a wallet</span>
            </button>`:h}
        ${r?l`
              <button
                class="provider ${s}"
                ?disabled=${this.busy}
                @click=${this.loginWithNostr}
              >
                ${lr}<span>Continue with Nostr</span>
              </button>
              ${this.renderNostrFallback()}
            `:h}
        ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
      </div>
    `)}frame(e){return this.variant!=="panel"?e:l`
      <div class="panel">
        <div class="head">
          <span class="mark" aria-hidden="true">O</span>
          <h1 class="title">Sign in to OpenApps</h1>
          <p class="desc">
            One account for every app in the suite. Optional — the apps work
            without it.
          </p>
        </div>
        <div class="body">${e}</div>
      </div>
    `}renderNostrFallback(){return this.nostrFallback==="bunker"?this.renderBunkerForm():this.nostrFallback==="nsec"?this.renderNsecForm():l`
      <button
        class="link"
        ?disabled=${this.busy}
        @click=${()=>{this.nostrFallback="bunker",this.nostrHint=null}}
      >
        No extension? Use a remote signer
      </button>
    `}renderBunkerForm(){return l`
      <form class="nsec" @submit=${this.loginWithBunker}>
        ${this.nostrHint?l`<p class="muted small">${this.nostrHint}</p>`:h}
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
        ${this.authUrl?l`<p class="muted small">
              Your signer needs approval:
              <a href=${this.authUrl} target="_blank" rel="noreferrer noopener"
                >open it</a
              >, then come back.
            </p>`:h}
        <div class="row">
          <button class="primary" type="submit" ?disabled=${this.busy}>
            ${this.busy?"Waiting for your signer…":"Connect signer"}
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
    `}renderNsecForm(){return l`
      <form class="nsec" @submit=${this.loginWithNsec}>
        ${this.nostrHint?l`<p class="muted small">${this.nostrHint}</p>`:h}
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
    `}renderSignedIn(e){var r;const t=e.display_name??((r=e.linked_accounts[0])==null?void 0:r.caip10)??e.id;return l`
      <div class="row">
        <span class="identity" title=${t}>${gr(t)}</span>
        <button ?disabled=${this.busy} @click=${this.logout}>Sign out</button>
      </div>
      ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
    `}},K.styles=[w.baseStyles,ne`
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
    `],K);D([v()],P.prototype,"me",void 0);D([v()],P.prototype,"enabled",void 0);D([N({type:Number,attribute:"signer-timeout"})],P.prototype,"signerTimeout",void 0);D([N({type:String})],P.prototype,"variant",void 0);D([v()],P.prototype,"nostrFallback",void 0);D([v()],P.prototype,"nostrHint",void 0);D([v()],P.prototype,"authUrl",void 0);P=D([ye("openapps-login")],P);function gr(n,e=10,t=6){return n.length<=e+t+1?n:`${n.slice(0,e)}…${n.slice(-t)}`}function ae(){return typeof location>"u"?void 0:new URLSearchParams(location.search).get("ref")??void 0}var se=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i};const Le={google:"Google",eip155:"Wallet",nostr:"Nostr"};var Y;let B=(Y=class extends w{constructor(){super(...arguments),this.me=null,this.enabled=null,this.pending=null,this.notice=null,this.blocked=null}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){if(await Promise.resolve(),this.handleLinkRedirect(),this.enabled||(this.enabled=await this.run(()=>this.sdk.auth.methods())??null),!this.sdk.isLoggedIn){this.me=null;return}this.me=await this.run(()=>this.sdk.auth.me())??null}linked(e){var t;return(((t=this.me)==null?void 0:t.linked_accounts)??[]).some(r=>r.namespace===e)}get connectable(){return["eip155","nostr"].filter(e=>{var t;return((t=this.enabled)==null?void 0:t[e])&&!this.linked(e)})}get canConnectGoogle(){var e;return(((e=this.enabled)==null?void 0:e.google)??!1)&&!this.linked("google")}async connectGoogle(e=!1){await this.run(async()=>{const t=`${location.origin}${location.pathname}${location.search}`,r=await this.sdk.auth.googleLinkStart(t,{merge:e});window.location.href=r})}handleLinkRedirect(){let e;try{e=this.sdk.auth.completeLinkRedirect()}catch{return}if(e)switch(e.status){case"linked":this.notice=e.merged?`Accounts combined — ${e.credits.toLocaleString()} credits moved across.`:"Google connected.",this.emit("openapps-identity-linked",e),E();break;case"conflict":this.pending={namespace:"google",other:{id:"",balance:e.balance}};break;case"blocked":this.blocked=e.message;break;case"error":this.error=e.message;break}}async connect(e){this.blocked=null,await this.run(async()=>{var i,a,o;const t=e==="eip155"?await We():void 0,r=await this.sdk.auth.linkChallenge(e,t),s=e==="eip155"?await ze(r.message,t):await je(r.message);try{const c=await this.sdk.auth.linkVerify(r.challenge_id,s);this.afterLink(c)}catch(c){if(c instanceof k&&(((i=c.detail)==null?void 0:i.code)==="merge_blocked_by_duplicate_namespace"||((a=c.detail)==null?void 0:a.code)==="namespace_already_linked")){this.blocked=c.message;return}if(c instanceof k&&((o=c.detail)==null?void 0:o.code)==="identity_belongs_to_another_account"){this.pending={namespace:e,other:c.detail.other_account};return}throw c}})}async confirmMerge(){const e=this.pending;if(e){if(e.namespace==="google"){this.pending=null,await this.connectGoogle(!0);return}await this.run(async()=>{const t=e.namespace==="eip155"?await We():void 0,r=await this.sdk.auth.linkChallenge(e.namespace,t),s=e.namespace==="eip155"?await ze(r.message,t):await je(r.message),i=await this.sdk.auth.linkVerify(r.challenge_id,s,{merge:!0});this.pending=null,this.afterLink(i)})}}afterLink(e){this.notice=e.merged?`Accounts combined — ${(e.credits_transferred??0).toLocaleString()} credits moved across.`:"Connected.",this.emit("openapps-identity-linked",e),E(),this.load()}async unlink(e){await this.run(async()=>{await this.sdk.auth.unlink(e),this.notice="Disconnected.",this.emit("openapps-identity-unlinked",{caip10:e}),await this.load()})}render(){if(!this.sdkOrNull)return l`<p class="muted">Loading…</p>`;if(!this.sdk.isLoggedIn)return l`<p class="muted">Sign in to manage your account.</p>`;if(!this.me)return l`<p class="muted">Loading…</p>`;if(this.pending)return this.renderMergePrompt(this.pending);const e=this.me.linked_accounts;return l`
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
          ${e.map(t=>l`
              <li>
                <span class="tag">${Le[t.namespace]??t.namespace}</span>
                <code title=${t.caip10}
                  >${mr(t.label??t.caip10)}</code
                >
                ${e.length>1?l`<button
                      class="link"
                      ?disabled=${this.busy}
                      @click=${()=>this.unlink(t.caip10)}
                    >
                      Disconnect
                    </button>`:l`<span class="muted small">only method</span>`}
              </li>
            `)}
        </ul>

        ${this.connectable.length||this.canConnectGoogle?l`
              <h3>Add another</h3>
              <div class="row">
                ${this.canConnectGoogle?l`<button ?disabled=${this.busy} @click=${()=>this.connectGoogle()}>
                      Connect Google
                    </button>`:h}
                ${this.connectable.map(t=>l`
                    <button ?disabled=${this.busy} @click=${()=>this.connect(t)}>
                      Connect ${Le[t]}
                    </button>
                  `)}
              </div>
              <p class="muted small">
                Connecting a method that is already on another account will offer to
                combine them, so you keep one balance and one history.
              </p>
            `:l`<p class="muted small">Every available method is connected.</p>`}

        ${this.blocked?l`<p class="warn" role="alert">${this.blocked}</p>`:h}
        ${this.notice?l`<p class="notice">${this.notice}</p>`:h}
        ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
      </div>
    `}renderMergePrompt(e){return l`
      <div class="card">
        <h3>Combine two accounts?</h3>
        <p>
          That ${Le[e.namespace]??e.namespace} identity already
          belongs to another account holding
          <strong>${e.other.balance.toLocaleString()} credits</strong>.
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
        ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
      </div>
    `}},Y.styles=[w.baseStyles,ne`
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
    `],Y);se([v()],B.prototype,"me",void 0);se([v()],B.prototype,"enabled",void 0);se([v()],B.prototype,"pending",void 0);se([v()],B.prototype,"notice",void 0);se([v()],B.prototype,"blocked",void 0);B=se([ye("openapps-account")],B);function mr(n,e=18,t=8){return n.length<=e+t+1?n:`${n.slice(0,e)}…${n.slice(-t)}`}var Oe=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i},Z,Q;let fe=(Q=class extends w{constructor(){super(...arguments);C(this,Z);this.pollSeconds=0,this.label="Credits",this.balance=null}connectedCallback(){super.connectedCallback(),this.refresh(),this.pollSeconds>0&&$(this,Z,setInterval(()=>void this.refresh(),this.pollSeconds*1e3))}disconnectedCallback(){m(this,Z)&&clearInterval(m(this,Z)),super.disconnectedCallback()}onSessionChange(){this.refresh()}async refresh(){const t=this.sdkOrNull;if(!(t!=null&&t.isLoggedIn)){this.balance=null;return}const r=await this.run(()=>t.credits.balance());r!==void 0&&(this.balance=r)}render(){return this.sdkOrNull?this.sdk.isLoggedIn?l`
      <span class="wrap">
        <span class="label muted">${this.label}</span>
        <span class="value" aria-live="polite"
          >${this.balance===null?"…":this.balance.toLocaleString()}</span
        >
      </span>
      ${this.error?l`<span class="error" role="alert">${this.error}</span>`:h}
    `:l`<span class="muted">Not signed in</span>`:l`<span class="muted">…</span>`}},Z=new WeakMap,Q.styles=[w.baseStyles,ne`
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
    `],Q);Oe([N({type:Number,attribute:"poll-seconds"})],fe.prototype,"pollSeconds",void 0);Oe([N({type:String})],fe.prototype,"label",void 0);Oe([v()],fe.prototype,"balance",void 0);fe=Oe([ye("openapps-credits")],fe);var q=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i};function ot(n){return new Date(n*1e3).toLocaleDateString(void 0,{day:"numeric",month:"short"})}var X;let I=(X=class extends w{constructor(){super(...arguments),this.info=null,this.earnings=null,this.referees=null,this.tab="link",this.copied=!1}connectedCallback(){super.connectedCallback(),this.load()}onSessionChange(){this.load()}async load(){const e=this.sdkOrNull;if(!(e!=null&&e.isLoggedIn)){this.info=null,this.earnings=null,this.referees=null;return}this.info=await this.run(()=>e.referral.code())??null,this.earnings=await this.run(()=>e.referral.earnings())??null,this.referees=await this.run(()=>e.referral.referees())??null}get link(){const e=this.inviteUrl??(typeof location>"u"?"":`${location.origin}${location.pathname}`);if(!this.info)return e;const t=e.includes("?")?"&":"?";return`${e}${t}ref=${encodeURIComponent(this.info.code)}`}async copy(){try{await navigator.clipboard.writeText(this.link),this.copied=!0,setTimeout(()=>this.copied=!1,2e3)}catch{this.error="Could not copy. Select the link and copy it manually."}}render(){var s,i;if(!this.sdkOrNull)return l`<p class="muted">Loading…</p>`;if(!this.sdk.isLoggedIn)return l`<p class="muted">Sign in to get your invite link.</p>`;if(!this.info)return l`<p class="muted">${this.error??"Loading…"}</p>`;const e=((s=this.referees)==null?void 0:s.referees)??[],t=((i=this.earnings)==null?void 0:i.entries)??[],r=[["link","Your link"],["referees",`Referees${e.length?` (${e.length})`:""}`],["earnings",`Earnings${t.length?` (${t.length})`:""}`]];return l`
      <div class="stack">
        <div class="tabs" role="tablist">
          ${r.map(([a,o])=>l`
              <button
                role="tab"
                aria-selected=${this.tab===a}
                class="tab ${this.tab===a?"on":""}"
                @click=${()=>this.tab=a}
              >
                ${o}
              </button>
            `)}
        </div>
        ${this.tab==="link"?this.renderLink():this.tab==="referees"?this.renderReferees(e):this.renderEarnings(t)}
        ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
      </div>
    `}renderLink(){var r,s,i,a;const e=((r=this.earnings)==null?void 0:r.total)??0,t=((s=this.earnings)==null?void 0:s.entries.length)??0;return l`
      <p class="desc">
        Share this link. When someone signs up through it and buys credits,
        you earn <strong>${(i=this.info)==null?void 0:i.bonus_percent}%</strong> of what they
        buy, as credits.
      </p>
      <code class="payload">${this.link}</code>
      <div class="row">
        <button @click=${this.copy}>${this.copied?"Copied":"Copy link"}</button>
        <span class="muted mono">${(a=this.info)==null?void 0:a.code}</span>
      </div>
      <div class="earned">
        <span class="eyebrow">Earned</span>
        <span class="total mono">${e.toLocaleString()}</span>
        <span class="caption">
          ${t===0?"No referred purchases yet.":`credits from ${t} purchase${t===1?"":"s"}`}
        </span>
      </div>
    `}renderReferees(e){return e.length===0?l`<p class="muted">
        Nobody has signed up through your link yet.
      </p>`:l`
      <p class="caption">
        Handles only — signing up through a link does not share someone's
        identity with you.
      </p>
      <div class="list">
        ${e.map(t=>l`
            <div class="item">
              <span class="mono handle">${t.handle}</span>
              <span class="grow caption">
                joined ${ot(t.joined_at)} ·
                ${t.purchases===0?"no purchases yet":`${t.purchases} purchase${t.purchases===1?"":"s"}`}
              </span>
              <span class="mono amount ${t.earned>0?"good":""}">
                ${t.earned>0?`+${t.earned.toLocaleString()}`:"—"}
              </span>
            </div>
          `)}
      </div>
    `}renderEarnings(e){return e.length===0?l`<p class="muted">
        No referral earnings yet. A bonus is credited when a referee's
        purchase settles.
      </p>`:l`
      <p class="caption">
        Each row is one bonus, credited in the same transaction as the
        referee's purchase — so this list and your balance cannot disagree.
      </p>
      <div class="list">
        ${e.map(t=>l`
            <div class="item">
              <span class="mono date">${ot(t.created_at)}</span>
              <span class="grow caption">
                ${t.referee??"unknown"}
                ${t.referee_credits?l` bought ${t.referee_credits.toLocaleString()} credits`:h}
              </span>
              <span class="mono amount good">+${t.amount.toLocaleString()}</span>
            </div>
          `)}
      </div>
    `}},X.styles=[w.baseStyles,ne`
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
    `],X);q([N({type:String,attribute:"invite-url"})],I.prototype,"inviteUrl",void 0);q([v()],I.prototype,"info",void 0);q([v()],I.prototype,"earnings",void 0);q([v()],I.prototype,"referees",void 0);q([v()],I.prototype,"tab",void 0);q([v()],I.prototype,"copied",void 0);I=q([ye("openapps-referral")],I);var V=function(n,e,t,r){var s=arguments.length,i=s<3?e:r===null?r=Object.getOwnPropertyDescriptor(e,t):r,a;if(typeof Reflect=="object"&&typeof Reflect.decorate=="function")i=Reflect.decorate(n,e,t,r);else for(var o=n.length-1;o>=0;o--)(a=n[o])&&(i=(s<3?a(i):s>3?a(e,t,i):a(e,t))||i);return s>3&&i&&Object.defineProperty(e,t,i),i},H,ee;let M=(ee=class extends w{constructor(){super(...arguments);C(this,H);this.rails="",this.packages=null,this.selected=null,this.instruction=null,this.topup=null,this.waiting=!1}connectedCallback(){super.connectedCallback(),this.load()}disconnectedCallback(){var t;(t=m(this,H))==null||t.abort(),super.disconnectedCallback()}onSessionChange(){if(!this.packages){this.load();return}this.requestUpdate()}async load(){if(!this.sdkOrNull)return;const t=await this.run(()=>this.sdk.payments.packages());t&&(this.packages=t)}get offeredRails(){if(!this.packages)return[];const t=["stripe","ethereum","lightning"].filter(s=>{var i,a;return(a=(i=this.packages)==null?void 0:i.rails)==null?void 0:a[s]}),r=this.rails.split(",").map(s=>s.trim()).filter(Boolean);return r.length?t.filter(s=>r.includes(s)):t}async start(t){const r=this.selected;r&&await this.run(async()=>{let s;switch(t){case"stripe":{const i=await this.sdk.payments.stripeCheckout(r.id);this.instruction={kind:"redirect"},window.location.href=i.checkout_url;return}case"ethereum":{const i=await this.sdk.payments.ethDepositAddress(r.id);s=i.topup_id,this.instruction={kind:"address",chain:i.chain,address:i.address,amount:i.expected_amount};break}case"lightning":{const i=await this.sdk.payments.lightningInvoice(r.id);s=i.topup_id,this.instruction={kind:"invoice",bolt11:i.bolt11,amountMsat:i.amount_msat};break}}this.watch(s,vr[t])})}async watch(t,r){var i;(i=m(this,H))==null||i.abort();const s=new AbortController;$(this,H,s),this.waiting=!0;try{const a=await this.sdk.payments.waitFor(t,{timeoutMs:r,signal:s.signal,onPoll:o=>{this.topup=o}});this.topup=a,a.status==="confirmed"&&(this.emit("openapps-topup",a),E())}catch(a){a instanceof Error&&a.name==="AbortError"||(this.error=kr(a))}finally{this.waiting=!1}}reset(){var t;(t=m(this,H))==null||t.abort(),this.selected=null,this.instruction=null,this.topup=null,this.error=null}render(){return this.sdkOrNull?this.sdk.isLoggedIn?this.packages?this.instruction?this.renderInstruction(this.instruction):this.selected?this.renderRails(this.selected):this.renderPackages(this.packages.packages??[]):l`<p class="muted">${this.error??"Loading packages…"}</p>`:l`<p class="muted">Sign in to buy credits.</p>`:l`<p class="muted">Loading…</p>`}renderPackages(t){return t.length===0?l`<p class="muted">No credit packages are configured.</p>`:l`
      <div class="grid">
        ${t.map(r=>l`
            <button class="package" @click=${()=>this.selected=r}>
              <span class="credits">
                ${r.credits.toLocaleString()} credits
              </span>
              <span class="perunit caption">${wr(r)}</span>
              <span class="price">${lt(r.usd_price)}</span>
            </button>
          `)}
      </div>
      ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
    `}renderRails(t){const r=this.offeredRails;return l`
      <p>
        <strong>${t.credits.toLocaleString()} credits</strong> —
        ${lt(t.usd_price)}
      </p>
      <div class="stack">
        ${r.map(s=>l`
            <button class="primary" ?disabled=${this.busy} @click=${()=>this.start(s)}>
              ${br[s]}
            </button>
          `)}
        ${r.length===0?l`<p class="muted">No payment methods are enabled.</p>`:h}
        <button @click=${this.reset}>Back</button>
      </div>
      ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
    `}renderInstruction(t){var s;const r=((s=this.topup)==null?void 0:s.status)??"pending";return r==="confirmed"?l`
        <span class="badge success"><span class="dot"></span>Confirmed</span>
        <p class="ok">Payment confirmed — credits added.</p>
        <button @click=${this.reset}>Buy more</button>
      `:r==="failed"||r==="expired"?l`
        <span class="badge danger"><span class="dot"></span>${r==="failed"?"Failed":"Expired"}</span>
        <p class="error" role="alert">This top-up ${r}. Nothing was charged.</p>
        <button @click=${this.reset}>Try again</button>
      `:l`
      ${t.kind==="redirect"?l`<p class="muted">Redirecting to checkout…</p>`:h}
      ${t.kind==="address"?l`
            <p>Send exactly <strong>${$r(t.amount,6)}</strong> USDC or
            USDT on <code>${t.chain}</code> to:</p>
            <code class="payload">${t.address}</code>
          `:h}
      ${t.kind==="invoice"?l`
            <p>Pay this Lightning invoice
            (<strong>${Math.ceil(t.amountMsat/1e3).toLocaleString()} sats</strong>):</p>
            <code class="payload">${t.bolt11}</code>
          `:h}
      ${t.kind!=="redirect"?l`
            <div class="row">
              <button @click=${()=>this.copy(yr(t))}>Copy</button>
              <button @click=${this.reset}>Cancel</button>
            </div>
            ${this.renderWaiting()}
          `:h}
      ${this.error?l`<p class="error" role="alert">${this.error}</p>`:h}
    `}renderWaiting(){var i,a;if(!this.waiting)return l`<p class="muted" aria-live="polite">Not watching for payment.</p>`;const t=(i=this.topup)==null?void 0:i.confirmations;if(t===void 0)return l`<p class="muted" aria-live="polite">Waiting for payment…</p>`;const r=(a=this.topup)==null?void 0:a.confirmations_required;if(r==null)return l`
        <p class="muted" aria-live="polite">
          Payment received — waiting for the network to finalise it.
        </p>
      `;const s=Math.min(t,r);return l`
      <p class="muted" aria-live="polite">
        Payment received — confirming (${s} of ${r}).
      </p>
      <progress
        class="confirms"
        max=${r}
        value=${s}
        aria-label="Confirmations"
      ></progress>
    `}async copy(t){try{await navigator.clipboard.writeText(t)}catch{this.error="Could not copy — select the text and copy it manually."}}},H=new WeakMap,ee.styles=[w.baseStyles,ne`
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
    `],ee);V([N({type:String})],M.prototype,"rails",void 0);V([v()],M.prototype,"packages",void 0);V([v()],M.prototype,"selected",void 0);V([v()],M.prototype,"instruction",void 0);V([v()],M.prototype,"topup",void 0);V([v()],M.prototype,"waiting",void 0);M=V([ye("openapps-buy")],M);const br={stripe:"Pay by card",ethereum:"Pay with USDC / USDT",lightning:"Pay with Lightning"},vr={stripe:void 0,lightning:void 0,ethereum:1800*1e3};function yr(n){return n.kind==="address"?n.address:n.kind==="invoice"?n.bolt11:""}function lt(n){return`$${(n/100).toFixed(2)}`}function wr(n){if(n.credits<=0)return"";const e=n.usd_price/n.credits;return`${e<1?e.toFixed(2):e.toFixed(1)}¢ each`}function $r(n,e){const t=10**e;return(n/t).toFixed(e).replace(/\.?0+$/,"")}function kr(n){const e=n instanceof Error?n.message:String(n);return e.includes("still pending")?"Still waiting on the network. Your credits will appear once the payment settles.":e}function _r(n,e,t,r){if(typeof e=="function"?n!==e||!r:!e.has(n))throw new TypeError("Cannot read private member from an object whose class did not declare it");return t==="m"?r:t==="a"?r.call(n):r?r.value:e.get(n)}function xr(n,e,t,r,s){if(typeof e=="function"?n!==e||!0:!e.has(n))throw new TypeError("Cannot write private member to an object whose class did not declare it");return e.set(n,t),t}var xe;const Tr="__TAURI_TO_IPC_KEY__";function Cr(n,e=!1){return window.__TAURI_INTERNALS__.transformCallback(n,e)}async function we(n,e={},t){return window.__TAURI_INTERNALS__.invoke(n,e,t)}class Rr{get rid(){return _r(this,xe,"f")}constructor(e){xe.set(this,void 0),xr(this,xe,e)}async close(){return we("plugin:resources|close",{rid:this.rid})}}xe=new WeakMap;var ct;(function(n){n.WINDOW_RESIZED="tauri://resize",n.WINDOW_MOVED="tauri://move",n.WINDOW_CLOSE_REQUESTED="tauri://close-requested",n.WINDOW_DESTROYED="tauri://destroyed",n.WINDOW_FOCUS="tauri://focus",n.WINDOW_BLUR="tauri://blur",n.WINDOW_SCALE_FACTOR_CHANGED="tauri://scale-change",n.WINDOW_THEME_CHANGED="tauri://theme-changed",n.WINDOW_CREATED="tauri://window-created",n.WINDOW_SUSPENDED="tauri://suspended",n.WINDOW_RESUMED="tauri://resumed",n.WEBVIEW_CREATED="tauri://webview-created",n.DRAG_ENTER="tauri://drag-enter",n.DRAG_OVER="tauri://drag-over",n.DRAG_DROP="tauri://drag-drop",n.DRAG_LEAVE="tauri://drag-leave"})(ct||(ct={}));async function xt(n,e){window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(n,e),await we("plugin:event|unlisten",{event:n,eventId:e})}async function Sr(n,e,t){var r;const s=typeof(t==null?void 0:t.target)=="string"?{kind:"AnyLabel",label:t.target}:(r=t==null?void 0:t.target)!==null&&r!==void 0?r:{kind:"Any"};return we("plugin:event|listen",{event:n,target:s,handler:Cr(e)}).then(i=>async()=>xt(n,i))}async function Ur(n,e,t){return Sr(n,r=>{xt(n,r.id),e(r)},t)}async function Lr(n,e){await we("plugin:event|emit",{event:n,payload:e})}async function Ir(n,e,t){await we("plugin:event|emit_to",{target:typeof n=="string"?{kind:"AnyLabel",label:n}:n,event:e,payload:t})}export{Rr as R,Tr as S,ct as T,Ir as a,Dt as b,It as c,Lr as e,Or as g,we as i,Sr as l,Ur as o};

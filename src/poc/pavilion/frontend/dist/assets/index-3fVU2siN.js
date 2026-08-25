(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const l of document.querySelectorAll('link[rel="modulepreload"]'))a(l);new MutationObserver(l=>{for(const c of l)if(c.type==="childList")for(const f of c.addedNodes)f.tagName==="LINK"&&f.rel==="modulepreload"&&a(f)}).observe(document,{childList:!0,subtree:!0});function n(l){const c={};return l.integrity&&(c.integrity=l.integrity),l.referrerPolicy&&(c.referrerPolicy=l.referrerPolicy),l.crossOrigin==="use-credentials"?c.credentials="include":l.crossOrigin==="anonymous"?c.credentials="omit":c.credentials="same-origin",c}function a(l){if(l.ep)return;l.ep=!0;const c=n(l);fetch(l.href,c)}})();var Vd={exports:{}},il={};/**
 * @license React
 * react-jsx-runtime.production.js
 *
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */var N_;function uM(){if(N_)return il;N_=1;var r=Symbol.for("react.transitional.element"),t=Symbol.for("react.fragment");function n(a,l,c){var f=null;if(c!==void 0&&(f=""+c),l.key!==void 0&&(f=""+l.key),"key"in l){c={};for(var d in l)d!=="key"&&(c[d]=l[d])}else c=l;return l=c.ref,{$$typeof:r,type:a,key:f,ref:l!==void 0?l:null,props:c}}return il.Fragment=t,il.jsx=n,il.jsxs=n,il}var D_;function fM(){return D_||(D_=1,Vd.exports=uM()),Vd.exports}var b=fM(),kd={exports:{}},he={};/**
 * @license React
 * react.production.js
 *
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */var U_;function dM(){if(U_)return he;U_=1;var r=Symbol.for("react.transitional.element"),t=Symbol.for("react.portal"),n=Symbol.for("react.fragment"),a=Symbol.for("react.strict_mode"),l=Symbol.for("react.profiler"),c=Symbol.for("react.consumer"),f=Symbol.for("react.context"),d=Symbol.for("react.forward_ref"),m=Symbol.for("react.suspense"),h=Symbol.for("react.memo"),g=Symbol.for("react.lazy"),_=Symbol.for("react.activity"),v=Symbol.iterator;function y(I){return I===null||typeof I!="object"?null:(I=v&&I[v]||I["@@iterator"],typeof I=="function"?I:null)}var E={isMounted:function(){return!1},enqueueForceUpdate:function(){},enqueueReplaceState:function(){},enqueueSetState:function(){}},A=Object.assign,S={};function x(I,Q,Mt){this.props=I,this.context=Q,this.refs=S,this.updater=Mt||E}x.prototype.isReactComponent={},x.prototype.setState=function(I,Q){if(typeof I!="object"&&typeof I!="function"&&I!=null)throw Error("takes an object of state variables to update or a function which returns an object of state variables.");this.updater.enqueueSetState(this,I,Q,"setState")},x.prototype.forceUpdate=function(I){this.updater.enqueueForceUpdate(this,I,"forceUpdate")};function w(){}w.prototype=x.prototype;function D(I,Q,Mt){this.props=I,this.context=Q,this.refs=S,this.updater=Mt||E}var U=D.prototype=new w;U.constructor=D,A(U,x.prototype),U.isPureReactComponent=!0;var G=Array.isArray;function O(){}var B={H:null,A:null,T:null,S:null},R=Object.prototype.hasOwnProperty;function z(I,Q,Mt){var Rt=Mt.ref;return{$$typeof:r,type:I,key:Q,ref:Rt!==void 0?Rt:null,props:Mt}}function K(I,Q){return z(I.type,Q,I.props)}function V(I){return typeof I=="object"&&I!==null&&I.$$typeof===r}function $(I){var Q={"=":"=0",":":"=2"};return"$"+I.replace(/[=:]/g,function(Mt){return Q[Mt]})}var ht=/\/+/g;function gt(I,Q){return typeof I=="object"&&I!==null&&I.key!=null?$(""+I.key):Q.toString(36)}function q(I){switch(I.status){case"fulfilled":return I.value;case"rejected":throw I.reason;default:switch(typeof I.status=="string"?I.then(O,O):(I.status="pending",I.then(function(Q){I.status==="pending"&&(I.status="fulfilled",I.value=Q)},function(Q){I.status==="pending"&&(I.status="rejected",I.reason=Q)})),I.status){case"fulfilled":return I.value;case"rejected":throw I.reason}}throw I}function P(I,Q,Mt,Rt,wt){var st=typeof I;(st==="undefined"||st==="boolean")&&(I=null);var bt=!1;if(I===null)bt=!0;else switch(st){case"bigint":case"string":case"number":bt=!0;break;case"object":switch(I.$$typeof){case r:case t:bt=!0;break;case g:return bt=I._init,P(bt(I._payload),Q,Mt,Rt,wt)}}if(bt)return wt=wt(I),bt=Rt===""?"."+gt(I,0):Rt,G(wt)?(Mt="",bt!=null&&(Mt=bt.replace(ht,"$&/")+"/"),P(wt,Q,Mt,"",function(re){return re})):wt!=null&&(V(wt)&&(wt=K(wt,Mt+(wt.key==null||I&&I.key===wt.key?"":(""+wt.key).replace(ht,"$&/")+"/")+bt)),Q.push(wt)),1;bt=0;var Tt=Rt===""?".":Rt+":";if(G(I))for(var Wt=0;Wt<I.length;Wt++)Rt=I[Wt],st=Tt+gt(Rt,Wt),bt+=P(Rt,Q,Mt,st,wt);else if(Wt=y(I),typeof Wt=="function")for(I=Wt.call(I),Wt=0;!(Rt=I.next()).done;)Rt=Rt.value,st=Tt+gt(Rt,Wt++),bt+=P(Rt,Q,Mt,st,wt);else if(st==="object"){if(typeof I.then=="function")return P(q(I),Q,Mt,Rt,wt);throw Q=String(I),Error("Objects are not valid as a React child (found: "+(Q==="[object Object]"?"object with keys {"+Object.keys(I).join(", ")+"}":Q)+"). If you meant to render a collection of children, use an array instead.")}return bt}function F(I,Q,Mt){if(I==null)return I;var Rt=[],wt=0;return P(I,Rt,"","",function(st){return Q.call(Mt,st,wt++)}),Rt}function ct(I){if(I._status===-1){var Q=I._result;Q=Q(),Q.then(function(Mt){(I._status===0||I._status===-1)&&(I._status=1,I._result=Mt)},function(Mt){(I._status===0||I._status===-1)&&(I._status=2,I._result=Mt)}),I._status===-1&&(I._status=0,I._result=Q)}if(I._status===1)return I._result.default;throw I._result}var J=typeof reportError=="function"?reportError:function(I){if(typeof window=="object"&&typeof window.ErrorEvent=="function"){var Q=new window.ErrorEvent("error",{bubbles:!0,cancelable:!0,message:typeof I=="object"&&I!==null&&typeof I.message=="string"?String(I.message):String(I),error:I});if(!window.dispatchEvent(Q))return}else if(typeof process=="object"&&typeof process.emit=="function"){process.emit("uncaughtException",I);return}console.error(I)},xt={map:F,forEach:function(I,Q,Mt){F(I,function(){Q.apply(this,arguments)},Mt)},count:function(I){var Q=0;return F(I,function(){Q++}),Q},toArray:function(I){return F(I,function(Q){return Q})||[]},only:function(I){if(!V(I))throw Error("React.Children.only expected to receive a single React element child.");return I}};return he.Activity=_,he.Children=xt,he.Component=x,he.Fragment=n,he.Profiler=l,he.PureComponent=D,he.StrictMode=a,he.Suspense=m,he.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE=B,he.__COMPILER_RUNTIME={__proto__:null,c:function(I){return B.H.useMemoCache(I)}},he.cache=function(I){return function(){return I.apply(null,arguments)}},he.cacheSignal=function(){return null},he.cloneElement=function(I,Q,Mt){if(I==null)throw Error("The argument must be a React element, but you passed "+I+".");var Rt=A({},I.props),wt=I.key;if(Q!=null)for(st in Q.key!==void 0&&(wt=""+Q.key),Q)!R.call(Q,st)||st==="key"||st==="__self"||st==="__source"||st==="ref"&&Q.ref===void 0||(Rt[st]=Q[st]);var st=arguments.length-2;if(st===1)Rt.children=Mt;else if(1<st){for(var bt=Array(st),Tt=0;Tt<st;Tt++)bt[Tt]=arguments[Tt+2];Rt.children=bt}return z(I.type,wt,Rt)},he.createContext=function(I){return I={$$typeof:f,_currentValue:I,_currentValue2:I,_threadCount:0,Provider:null,Consumer:null},I.Provider=I,I.Consumer={$$typeof:c,_context:I},I},he.createElement=function(I,Q,Mt){var Rt,wt={},st=null;if(Q!=null)for(Rt in Q.key!==void 0&&(st=""+Q.key),Q)R.call(Q,Rt)&&Rt!=="key"&&Rt!=="__self"&&Rt!=="__source"&&(wt[Rt]=Q[Rt]);var bt=arguments.length-2;if(bt===1)wt.children=Mt;else if(1<bt){for(var Tt=Array(bt),Wt=0;Wt<bt;Wt++)Tt[Wt]=arguments[Wt+2];wt.children=Tt}if(I&&I.defaultProps)for(Rt in bt=I.defaultProps,bt)wt[Rt]===void 0&&(wt[Rt]=bt[Rt]);return z(I,st,wt)},he.createRef=function(){return{current:null}},he.forwardRef=function(I){return{$$typeof:d,render:I}},he.isValidElement=V,he.lazy=function(I){return{$$typeof:g,_payload:{_status:-1,_result:I},_init:ct}},he.memo=function(I,Q){return{$$typeof:h,type:I,compare:Q===void 0?null:Q}},he.startTransition=function(I){var Q=B.T,Mt={};B.T=Mt;try{var Rt=I(),wt=B.S;wt!==null&&wt(Mt,Rt),typeof Rt=="object"&&Rt!==null&&typeof Rt.then=="function"&&Rt.then(O,J)}catch(st){J(st)}finally{Q!==null&&Mt.types!==null&&(Q.types=Mt.types),B.T=Q}},he.unstable_useCacheRefresh=function(){return B.H.useCacheRefresh()},he.use=function(I){return B.H.use(I)},he.useActionState=function(I,Q,Mt){return B.H.useActionState(I,Q,Mt)},he.useCallback=function(I,Q){return B.H.useCallback(I,Q)},he.useContext=function(I){return B.H.useContext(I)},he.useDebugValue=function(){},he.useDeferredValue=function(I,Q){return B.H.useDeferredValue(I,Q)},he.useEffect=function(I,Q){return B.H.useEffect(I,Q)},he.useEffectEvent=function(I){return B.H.useEffectEvent(I)},he.useId=function(){return B.H.useId()},he.useImperativeHandle=function(I,Q,Mt){return B.H.useImperativeHandle(I,Q,Mt)},he.useInsertionEffect=function(I,Q){return B.H.useInsertionEffect(I,Q)},he.useLayoutEffect=function(I,Q){return B.H.useLayoutEffect(I,Q)},he.useMemo=function(I,Q){return B.H.useMemo(I,Q)},he.useOptimistic=function(I,Q){return B.H.useOptimistic(I,Q)},he.useReducer=function(I,Q,Mt){return B.H.useReducer(I,Q,Mt)},he.useRef=function(I){return B.H.useRef(I)},he.useState=function(I){return B.H.useState(I)},he.useSyncExternalStore=function(I,Q,Mt){return B.H.useSyncExternalStore(I,Q,Mt)},he.useTransition=function(){return B.H.useTransition()},he.version="19.2.5",he}var L_;function Up(){return L_||(L_=1,kd.exports=dM()),kd.exports}var ut=Up(),jd={exports:{}},al={},Xd={exports:{}},Wd={};/**
 * @license React
 * scheduler.production.js
 *
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */var O_;function hM(){return O_||(O_=1,(function(r){function t(P,F){var ct=P.length;P.push(F);t:for(;0<ct;){var J=ct-1>>>1,xt=P[J];if(0<l(xt,F))P[J]=F,P[ct]=xt,ct=J;else break t}}function n(P){return P.length===0?null:P[0]}function a(P){if(P.length===0)return null;var F=P[0],ct=P.pop();if(ct!==F){P[0]=ct;t:for(var J=0,xt=P.length,I=xt>>>1;J<I;){var Q=2*(J+1)-1,Mt=P[Q],Rt=Q+1,wt=P[Rt];if(0>l(Mt,ct))Rt<xt&&0>l(wt,Mt)?(P[J]=wt,P[Rt]=ct,J=Rt):(P[J]=Mt,P[Q]=ct,J=Q);else if(Rt<xt&&0>l(wt,ct))P[J]=wt,P[Rt]=ct,J=Rt;else break t}}return F}function l(P,F){var ct=P.sortIndex-F.sortIndex;return ct!==0?ct:P.id-F.id}if(r.unstable_now=void 0,typeof performance=="object"&&typeof performance.now=="function"){var c=performance;r.unstable_now=function(){return c.now()}}else{var f=Date,d=f.now();r.unstable_now=function(){return f.now()-d}}var m=[],h=[],g=1,_=null,v=3,y=!1,E=!1,A=!1,S=!1,x=typeof setTimeout=="function"?setTimeout:null,w=typeof clearTimeout=="function"?clearTimeout:null,D=typeof setImmediate<"u"?setImmediate:null;function U(P){for(var F=n(h);F!==null;){if(F.callback===null)a(h);else if(F.startTime<=P)a(h),F.sortIndex=F.expirationTime,t(m,F);else break;F=n(h)}}function G(P){if(A=!1,U(P),!E)if(n(m)!==null)E=!0,O||(O=!0,$());else{var F=n(h);F!==null&&q(G,F.startTime-P)}}var O=!1,B=-1,R=5,z=-1;function K(){return S?!0:!(r.unstable_now()-z<R)}function V(){if(S=!1,O){var P=r.unstable_now();z=P;var F=!0;try{t:{E=!1,A&&(A=!1,w(B),B=-1),y=!0;var ct=v;try{e:{for(U(P),_=n(m);_!==null&&!(_.expirationTime>P&&K());){var J=_.callback;if(typeof J=="function"){_.callback=null,v=_.priorityLevel;var xt=J(_.expirationTime<=P);if(P=r.unstable_now(),typeof xt=="function"){_.callback=xt,U(P),F=!0;break e}_===n(m)&&a(m),U(P)}else a(m);_=n(m)}if(_!==null)F=!0;else{var I=n(h);I!==null&&q(G,I.startTime-P),F=!1}}break t}finally{_=null,v=ct,y=!1}F=void 0}}finally{F?$():O=!1}}}var $;if(typeof D=="function")$=function(){D(V)};else if(typeof MessageChannel<"u"){var ht=new MessageChannel,gt=ht.port2;ht.port1.onmessage=V,$=function(){gt.postMessage(null)}}else $=function(){x(V,0)};function q(P,F){B=x(function(){P(r.unstable_now())},F)}r.unstable_IdlePriority=5,r.unstable_ImmediatePriority=1,r.unstable_LowPriority=4,r.unstable_NormalPriority=3,r.unstable_Profiling=null,r.unstable_UserBlockingPriority=2,r.unstable_cancelCallback=function(P){P.callback=null},r.unstable_forceFrameRate=function(P){0>P||125<P?console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported"):R=0<P?Math.floor(1e3/P):5},r.unstable_getCurrentPriorityLevel=function(){return v},r.unstable_next=function(P){switch(v){case 1:case 2:case 3:var F=3;break;default:F=v}var ct=v;v=F;try{return P()}finally{v=ct}},r.unstable_requestPaint=function(){S=!0},r.unstable_runWithPriority=function(P,F){switch(P){case 1:case 2:case 3:case 4:case 5:break;default:P=3}var ct=v;v=P;try{return F()}finally{v=ct}},r.unstable_scheduleCallback=function(P,F,ct){var J=r.unstable_now();switch(typeof ct=="object"&&ct!==null?(ct=ct.delay,ct=typeof ct=="number"&&0<ct?J+ct:J):ct=J,P){case 1:var xt=-1;break;case 2:xt=250;break;case 5:xt=1073741823;break;case 4:xt=1e4;break;default:xt=5e3}return xt=ct+xt,P={id:g++,callback:F,priorityLevel:P,startTime:ct,expirationTime:xt,sortIndex:-1},ct>J?(P.sortIndex=ct,t(h,P),n(m)===null&&P===n(h)&&(A?(w(B),B=-1):A=!0,q(G,ct-J))):(P.sortIndex=xt,t(m,P),E||y||(E=!0,O||(O=!0,$()))),P},r.unstable_shouldYield=K,r.unstable_wrapCallback=function(P){var F=v;return function(){var ct=v;v=F;try{return P.apply(this,arguments)}finally{v=ct}}}})(Wd)),Wd}var P_;function pM(){return P_||(P_=1,Xd.exports=hM()),Xd.exports}var qd={exports:{}},Bn={};/**
 * @license React
 * react-dom.production.js
 *
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */var I_;function mM(){if(I_)return Bn;I_=1;var r=Up();function t(m){var h="https://react.dev/errors/"+m;if(1<arguments.length){h+="?args[]="+encodeURIComponent(arguments[1]);for(var g=2;g<arguments.length;g++)h+="&args[]="+encodeURIComponent(arguments[g])}return"Minified React error #"+m+"; visit "+h+" for the full message or use the non-minified dev environment for full errors and additional helpful warnings."}function n(){}var a={d:{f:n,r:function(){throw Error(t(522))},D:n,C:n,L:n,m:n,X:n,S:n,M:n},p:0,findDOMNode:null},l=Symbol.for("react.portal");function c(m,h,g){var _=3<arguments.length&&arguments[3]!==void 0?arguments[3]:null;return{$$typeof:l,key:_==null?null:""+_,children:m,containerInfo:h,implementation:g}}var f=r.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;function d(m,h){if(m==="font")return"";if(typeof h=="string")return h==="use-credentials"?h:""}return Bn.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE=a,Bn.createPortal=function(m,h){var g=2<arguments.length&&arguments[2]!==void 0?arguments[2]:null;if(!h||h.nodeType!==1&&h.nodeType!==9&&h.nodeType!==11)throw Error(t(299));return c(m,h,null,g)},Bn.flushSync=function(m){var h=f.T,g=a.p;try{if(f.T=null,a.p=2,m)return m()}finally{f.T=h,a.p=g,a.d.f()}},Bn.preconnect=function(m,h){typeof m=="string"&&(h?(h=h.crossOrigin,h=typeof h=="string"?h==="use-credentials"?h:"":void 0):h=null,a.d.C(m,h))},Bn.prefetchDNS=function(m){typeof m=="string"&&a.d.D(m)},Bn.preinit=function(m,h){if(typeof m=="string"&&h&&typeof h.as=="string"){var g=h.as,_=d(g,h.crossOrigin),v=typeof h.integrity=="string"?h.integrity:void 0,y=typeof h.fetchPriority=="string"?h.fetchPriority:void 0;g==="style"?a.d.S(m,typeof h.precedence=="string"?h.precedence:void 0,{crossOrigin:_,integrity:v,fetchPriority:y}):g==="script"&&a.d.X(m,{crossOrigin:_,integrity:v,fetchPriority:y,nonce:typeof h.nonce=="string"?h.nonce:void 0})}},Bn.preinitModule=function(m,h){if(typeof m=="string")if(typeof h=="object"&&h!==null){if(h.as==null||h.as==="script"){var g=d(h.as,h.crossOrigin);a.d.M(m,{crossOrigin:g,integrity:typeof h.integrity=="string"?h.integrity:void 0,nonce:typeof h.nonce=="string"?h.nonce:void 0})}}else h==null&&a.d.M(m)},Bn.preload=function(m,h){if(typeof m=="string"&&typeof h=="object"&&h!==null&&typeof h.as=="string"){var g=h.as,_=d(g,h.crossOrigin);a.d.L(m,g,{crossOrigin:_,integrity:typeof h.integrity=="string"?h.integrity:void 0,nonce:typeof h.nonce=="string"?h.nonce:void 0,type:typeof h.type=="string"?h.type:void 0,fetchPriority:typeof h.fetchPriority=="string"?h.fetchPriority:void 0,referrerPolicy:typeof h.referrerPolicy=="string"?h.referrerPolicy:void 0,imageSrcSet:typeof h.imageSrcSet=="string"?h.imageSrcSet:void 0,imageSizes:typeof h.imageSizes=="string"?h.imageSizes:void 0,media:typeof h.media=="string"?h.media:void 0})}},Bn.preloadModule=function(m,h){if(typeof m=="string")if(h){var g=d(h.as,h.crossOrigin);a.d.m(m,{as:typeof h.as=="string"&&h.as!=="script"?h.as:void 0,crossOrigin:g,integrity:typeof h.integrity=="string"?h.integrity:void 0})}else a.d.m(m)},Bn.requestFormReset=function(m){a.d.r(m)},Bn.unstable_batchedUpdates=function(m,h){return m(h)},Bn.useFormState=function(m,h,g){return f.H.useFormState(m,h,g)},Bn.useFormStatus=function(){return f.H.useHostTransitionStatus()},Bn.version="19.2.5",Bn}var z_;function gM(){if(z_)return qd.exports;z_=1;function r(){if(!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__>"u"||typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE!="function"))try{__REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r)}catch(t){console.error(t)}}return r(),qd.exports=mM(),qd.exports}/**
 * @license React
 * react-dom-client.production.js
 *
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */var B_;function _M(){if(B_)return al;B_=1;var r=pM(),t=Up(),n=gM();function a(e){var i="https://react.dev/errors/"+e;if(1<arguments.length){i+="?args[]="+encodeURIComponent(arguments[1]);for(var s=2;s<arguments.length;s++)i+="&args[]="+encodeURIComponent(arguments[s])}return"Minified React error #"+e+"; visit "+i+" for the full message or use the non-minified dev environment for full errors and additional helpful warnings."}function l(e){return!(!e||e.nodeType!==1&&e.nodeType!==9&&e.nodeType!==11)}function c(e){var i=e,s=e;if(e.alternate)for(;i.return;)i=i.return;else{e=i;do i=e,(i.flags&4098)!==0&&(s=i.return),e=i.return;while(e)}return i.tag===3?s:null}function f(e){if(e.tag===13){var i=e.memoizedState;if(i===null&&(e=e.alternate,e!==null&&(i=e.memoizedState)),i!==null)return i.dehydrated}return null}function d(e){if(e.tag===31){var i=e.memoizedState;if(i===null&&(e=e.alternate,e!==null&&(i=e.memoizedState)),i!==null)return i.dehydrated}return null}function m(e){if(c(e)!==e)throw Error(a(188))}function h(e){var i=e.alternate;if(!i){if(i=c(e),i===null)throw Error(a(188));return i!==e?null:e}for(var s=e,o=i;;){var u=s.return;if(u===null)break;var p=u.alternate;if(p===null){if(o=u.return,o!==null){s=o;continue}break}if(u.child===p.child){for(p=u.child;p;){if(p===s)return m(u),e;if(p===o)return m(u),i;p=p.sibling}throw Error(a(188))}if(s.return!==o.return)s=u,o=p;else{for(var M=!1,N=u.child;N;){if(N===s){M=!0,s=u,o=p;break}if(N===o){M=!0,o=u,s=p;break}N=N.sibling}if(!M){for(N=p.child;N;){if(N===s){M=!0,s=p,o=u;break}if(N===o){M=!0,o=p,s=u;break}N=N.sibling}if(!M)throw Error(a(189))}}if(s.alternate!==o)throw Error(a(190))}if(s.tag!==3)throw Error(a(188));return s.stateNode.current===s?e:i}function g(e){var i=e.tag;if(i===5||i===26||i===27||i===6)return e;for(e=e.child;e!==null;){if(i=g(e),i!==null)return i;e=e.sibling}return null}var _=Object.assign,v=Symbol.for("react.element"),y=Symbol.for("react.transitional.element"),E=Symbol.for("react.portal"),A=Symbol.for("react.fragment"),S=Symbol.for("react.strict_mode"),x=Symbol.for("react.profiler"),w=Symbol.for("react.consumer"),D=Symbol.for("react.context"),U=Symbol.for("react.forward_ref"),G=Symbol.for("react.suspense"),O=Symbol.for("react.suspense_list"),B=Symbol.for("react.memo"),R=Symbol.for("react.lazy"),z=Symbol.for("react.activity"),K=Symbol.for("react.memo_cache_sentinel"),V=Symbol.iterator;function $(e){return e===null||typeof e!="object"?null:(e=V&&e[V]||e["@@iterator"],typeof e=="function"?e:null)}var ht=Symbol.for("react.client.reference");function gt(e){if(e==null)return null;if(typeof e=="function")return e.$$typeof===ht?null:e.displayName||e.name||null;if(typeof e=="string")return e;switch(e){case A:return"Fragment";case x:return"Profiler";case S:return"StrictMode";case G:return"Suspense";case O:return"SuspenseList";case z:return"Activity"}if(typeof e=="object")switch(e.$$typeof){case E:return"Portal";case D:return e.displayName||"Context";case w:return(e._context.displayName||"Context")+".Consumer";case U:var i=e.render;return e=e.displayName,e||(e=i.displayName||i.name||"",e=e!==""?"ForwardRef("+e+")":"ForwardRef"),e;case B:return i=e.displayName||null,i!==null?i:gt(e.type)||"Memo";case R:i=e._payload,e=e._init;try{return gt(e(i))}catch{}}return null}var q=Array.isArray,P=t.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE,F=n.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE,ct={pending:!1,data:null,method:null,action:null},J=[],xt=-1;function I(e){return{current:e}}function Q(e){0>xt||(e.current=J[xt],J[xt]=null,xt--)}function Mt(e,i){xt++,J[xt]=e.current,e.current=i}var Rt=I(null),wt=I(null),st=I(null),bt=I(null);function Tt(e,i){switch(Mt(st,i),Mt(wt,e),Mt(Rt,null),i.nodeType){case 9:case 11:e=(e=i.documentElement)&&(e=e.namespaceURI)?$0(e):0;break;default:if(e=i.tagName,i=i.namespaceURI)i=$0(i),e=t_(i,e);else switch(e){case"svg":e=1;break;case"math":e=2;break;default:e=0}}Q(Rt),Mt(Rt,e)}function Wt(){Q(Rt),Q(wt),Q(st)}function re(e){e.memoizedState!==null&&Mt(bt,e);var i=Rt.current,s=t_(i,e.type);i!==s&&(Mt(wt,e),Mt(Rt,s))}function ie(e){wt.current===e&&(Q(Rt),Q(wt)),bt.current===e&&(Q(bt),$o._currentValue=ct)}var Nt,Ht;function Ut(e){if(Nt===void 0)try{throw Error()}catch(s){var i=s.stack.trim().match(/\n( *(at )?)/);Nt=i&&i[1]||"",Ht=-1<s.stack.indexOf(`
    at`)?" (<anonymous>)":-1<s.stack.indexOf("@")?"@unknown:0:0":""}return`
`+Nt+e+Ht}var Gt=!1;function ne(e,i){if(!e||Gt)return"";Gt=!0;var s=Error.prepareStackTrace;Error.prepareStackTrace=void 0;try{var o={DetermineComponentFrameRoot:function(){try{if(i){var St=function(){throw Error()};if(Object.defineProperty(St.prototype,"props",{set:function(){throw Error()}}),typeof Reflect=="object"&&Reflect.construct){try{Reflect.construct(St,[])}catch(ft){var lt=ft}Reflect.construct(e,[],St)}else{try{St.call()}catch(ft){lt=ft}e.call(St.prototype)}}else{try{throw Error()}catch(ft){lt=ft}(St=e())&&typeof St.catch=="function"&&St.catch(function(){})}}catch(ft){if(ft&&lt&&typeof ft.stack=="string")return[ft.stack,lt.stack]}return[null,null]}};o.DetermineComponentFrameRoot.displayName="DetermineComponentFrameRoot";var u=Object.getOwnPropertyDescriptor(o.DetermineComponentFrameRoot,"name");u&&u.configurable&&Object.defineProperty(o.DetermineComponentFrameRoot,"name",{value:"DetermineComponentFrameRoot"});var p=o.DetermineComponentFrameRoot(),M=p[0],N=p[1];if(M&&N){var H=M.split(`
`),nt=N.split(`
`);for(u=o=0;o<H.length&&!H[o].includes("DetermineComponentFrameRoot");)o++;for(;u<nt.length&&!nt[u].includes("DetermineComponentFrameRoot");)u++;if(o===H.length||u===nt.length)for(o=H.length-1,u=nt.length-1;1<=o&&0<=u&&H[o]!==nt[u];)u--;for(;1<=o&&0<=u;o--,u--)if(H[o]!==nt[u]){if(o!==1||u!==1)do if(o--,u--,0>u||H[o]!==nt[u]){var mt=`
`+H[o].replace(" at new "," at ");return e.displayName&&mt.includes("<anonymous>")&&(mt=mt.replace("<anonymous>",e.displayName)),mt}while(1<=o&&0<=u);break}}}finally{Gt=!1,Error.prepareStackTrace=s}return(s=e?e.displayName||e.name:"")?Ut(s):""}function Me(e,i){switch(e.tag){case 26:case 27:case 5:return Ut(e.type);case 16:return Ut("Lazy");case 13:return e.child!==i&&i!==null?Ut("Suspense Fallback"):Ut("Suspense");case 19:return Ut("SuspenseList");case 0:case 15:return ne(e.type,!1);case 11:return ne(e.type.render,!1);case 1:return ne(e.type,!0);case 31:return Ut("Activity");default:return""}}function le(e){try{var i="",s=null;do i+=Me(e,s),s=e,e=e.return;while(e);return i}catch(o){return`
Error generating stack: `+o.message+`
`+o.stack}}var Ue=Object.prototype.hasOwnProperty,W=r.unstable_scheduleCallback,We=r.unstable_cancelCallback,ye=r.unstable_shouldYield,qe=r.unstable_requestPaint,Dt=r.unstable_now,an=r.unstable_getCurrentPriorityLevel,L=r.unstable_ImmediatePriority,T=r.unstable_UserBlockingPriority,tt=r.unstable_NormalPriority,yt=r.unstable_LowPriority,At=r.unstable_IdlePriority,Lt=r.log,zt=r.unstable_setDisableYieldValue,dt=null,pt=null;function Bt(e){if(typeof Lt=="function"&&zt(e),pt&&typeof pt.setStrictMode=="function")try{pt.setStrictMode(dt,e)}catch{}}var Ft=Math.clz32?Math.clz32:fe,Pt=Math.log,Ot=Math.LN2;function fe(e){return e>>>=0,e===0?32:31-(Pt(e)/Ot|0)|0}var de=256,be=262144,j=4194304;function Ct(e){var i=e&42;if(i!==0)return i;switch(e&-e){case 1:return 1;case 2:return 2;case 4:return 4;case 8:return 8;case 16:return 16;case 32:return 32;case 64:return 64;case 128:return 128;case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:return e&261888;case 262144:case 524288:case 1048576:case 2097152:return e&3932160;case 4194304:case 8388608:case 16777216:case 33554432:return e&62914560;case 67108864:return 67108864;case 134217728:return 134217728;case 268435456:return 268435456;case 536870912:return 536870912;case 1073741824:return 0;default:return e}}function _t(e,i,s){var o=e.pendingLanes;if(o===0)return 0;var u=0,p=e.suspendedLanes,M=e.pingedLanes;e=e.warmLanes;var N=o&134217727;return N!==0?(o=N&~p,o!==0?u=Ct(o):(M&=N,M!==0?u=Ct(M):s||(s=N&~e,s!==0&&(u=Ct(s))))):(N=o&~p,N!==0?u=Ct(N):M!==0?u=Ct(M):s||(s=o&~e,s!==0&&(u=Ct(s)))),u===0?0:i!==0&&i!==u&&(i&p)===0&&(p=u&-u,s=i&-i,p>=s||p===32&&(s&4194048)!==0)?i:u}function jt(e,i){return(e.pendingLanes&~(e.suspendedLanes&~e.pingedLanes)&i)===0}function It(e,i){switch(e){case 1:case 2:case 4:case 8:case 64:return i+250;case 16:case 32:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:return i+5e3;case 4194304:case 8388608:case 16777216:case 33554432:return-1;case 67108864:case 134217728:case 268435456:case 536870912:case 1073741824:return-1;default:return-1}}function Et(){var e=j;return j<<=1,(j&62914560)===0&&(j=4194304),e}function Jt(e){for(var i=[],s=0;31>s;s++)i.push(e);return i}function ue(e,i){e.pendingLanes|=i,i!==268435456&&(e.suspendedLanes=0,e.pingedLanes=0,e.warmLanes=0)}function ln(e,i,s,o,u,p){var M=e.pendingLanes;e.pendingLanes=s,e.suspendedLanes=0,e.pingedLanes=0,e.warmLanes=0,e.expiredLanes&=s,e.entangledLanes&=s,e.errorRecoveryDisabledLanes&=s,e.shellSuspendCounter=0;var N=e.entanglements,H=e.expirationTimes,nt=e.hiddenUpdates;for(s=M&~s;0<s;){var mt=31-Ft(s),St=1<<mt;N[mt]=0,H[mt]=-1;var lt=nt[mt];if(lt!==null)for(nt[mt]=null,mt=0;mt<lt.length;mt++){var ft=lt[mt];ft!==null&&(ft.lane&=-536870913)}s&=~St}o!==0&&Ie(e,o,0),p!==0&&u===0&&e.tag!==0&&(e.suspendedLanes|=p&~(M&~i))}function Ie(e,i,s){e.pendingLanes|=i,e.suspendedLanes&=~i;var o=31-Ft(i);e.entangledLanes|=i,e.entanglements[o]=e.entanglements[o]|1073741824|s&261930}function mi(e,i){var s=e.entangledLanes|=i;for(e=e.entanglements;s;){var o=31-Ft(s),u=1<<o;u&i|e[o]&i&&(e[o]|=i),s&=~u}}function ei(e,i){var s=i&-i;return s=(s&42)!==0?1:ys(s),(s&(e.suspendedLanes|i))!==0?0:s}function ys(e){switch(e){case 2:e=1;break;case 8:e=4;break;case 32:e=16;break;case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:case 4194304:case 8388608:case 16777216:case 33554432:e=128;break;case 268435456:e=134217728;break;default:e=0}return e}function uo(e){return e&=-e,2<e?8<e?(e&134217727)!==0?32:268435456:8:2}function fo(){var e=F.p;return e!==0?e:(e=window.event,e===void 0?32:b_(e.type))}function ho(e,i){var s=F.p;try{return F.p=e,i()}finally{F.p=s}}var In=Math.random().toString(36).slice(2),dn="__reactFiber$"+In,Cn="__reactProps$"+In,ia="__reactContainer$"+In,Oa="__reactEvents$"+In,Cl="__reactListeners$"+In,nr="__reactHandles$"+In,po="__reactResources$"+In,Pa="__reactMarker$"+In;function mo(e){delete e[dn],delete e[Cn],delete e[Oa],delete e[Cl],delete e[nr]}function Ia(e){var i=e[dn];if(i)return i;for(var s=e.parentNode;s;){if(i=s[ia]||s[dn]){if(s=i.alternate,i.child!==null||s!==null&&s.child!==null)for(e=o_(e);e!==null;){if(s=e[dn])return s;e=o_(e)}return i}e=s,s=e.parentNode}return null}function za(e){if(e=e[dn]||e[ia]){var i=e.tag;if(i===5||i===6||i===13||i===31||i===26||i===27||i===3)return e}return null}function Ss(e){var i=e.tag;if(i===5||i===26||i===27||i===6)return e.stateNode;throw Error(a(33))}function Ba(e){var i=e[po];return i||(i=e[po]={hoistableStyles:new Map,hoistableScripts:new Map}),i}function mn(e){e[Pa]=!0}var Nl=new Set,C={};function Y(e,i){ot(e,i),ot(e+"Capture",i)}function ot(e,i){for(C[e]=i,e=0;e<i.length;e++)Nl.add(i[e])}var it=RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"),at={},kt={};function Yt(e){return Ue.call(kt,e)?!0:Ue.call(at,e)?!1:it.test(e)?kt[e]=!0:(at[e]=!0,!1)}function Vt(e,i,s){if(Yt(i))if(s===null)e.removeAttribute(i);else{switch(typeof s){case"undefined":case"function":case"symbol":e.removeAttribute(i);return;case"boolean":var o=i.toLowerCase().slice(0,5);if(o!=="data-"&&o!=="aria-"){e.removeAttribute(i);return}}e.setAttribute(i,""+s)}}function Kt(e,i,s){if(s===null)e.removeAttribute(i);else{switch(typeof s){case"undefined":case"function":case"symbol":case"boolean":e.removeAttribute(i);return}e.setAttribute(i,""+s)}}function Zt(e,i,s,o){if(o===null)e.removeAttribute(s);else{switch(typeof o){case"undefined":case"function":case"symbol":case"boolean":e.removeAttribute(s);return}e.setAttributeNS(i,s,""+o)}}function ae(e){switch(typeof e){case"bigint":case"boolean":case"number":case"string":case"undefined":return e;case"object":return e;default:return""}}function me(e){var i=e.type;return(e=e.nodeName)&&e.toLowerCase()==="input"&&(i==="checkbox"||i==="radio")}function te(e,i,s){var o=Object.getOwnPropertyDescriptor(e.constructor.prototype,i);if(!e.hasOwnProperty(i)&&typeof o<"u"&&typeof o.get=="function"&&typeof o.set=="function"){var u=o.get,p=o.set;return Object.defineProperty(e,i,{configurable:!0,get:function(){return u.call(this)},set:function(M){s=""+M,p.call(this,M)}}),Object.defineProperty(e,i,{enumerable:o.enumerable}),{getValue:function(){return s},setValue:function(M){s=""+M},stopTracking:function(){e._valueTracker=null,delete e[i]}}}}function Le(e){if(!e._valueTracker){var i=me(e)?"checked":"value";e._valueTracker=te(e,i,""+e[i])}}function sn(e){if(!e)return!1;var i=e._valueTracker;if(!i)return!0;var s=i.getValue(),o="";return e&&(o=me(e)?e.checked?"true":"false":e.value),e=o,e!==s?(i.setValue(e),!0):!1}function Qe(e){if(e=e||(typeof document<"u"?document:void 0),typeof e>"u")return null;try{return e.activeElement||e.body}catch{return e.body}}var Fe=/[\n"\\]/g;function He(e){return e.replace(Fe,function(i){return"\\"+i.charCodeAt(0).toString(16)+" "})}function qt(e,i,s,o,u,p,M,N){e.name="",M!=null&&typeof M!="function"&&typeof M!="symbol"&&typeof M!="boolean"?e.type=M:e.removeAttribute("type"),i!=null?M==="number"?(i===0&&e.value===""||e.value!=i)&&(e.value=""+ae(i)):e.value!==""+ae(i)&&(e.value=""+ae(i)):M!=="submit"&&M!=="reset"||e.removeAttribute("value"),i!=null?Ee(e,M,ae(i)):s!=null?Ee(e,M,ae(s)):o!=null&&e.removeAttribute("value"),u==null&&p!=null&&(e.defaultChecked=!!p),u!=null&&(e.checked=u&&typeof u!="function"&&typeof u!="symbol"),N!=null&&typeof N!="function"&&typeof N!="symbol"&&typeof N!="boolean"?e.name=""+ae(N):e.removeAttribute("name")}function zn(e,i,s,o,u,p,M,N){if(p!=null&&typeof p!="function"&&typeof p!="symbol"&&typeof p!="boolean"&&(e.type=p),i!=null||s!=null){if(!(p!=="submit"&&p!=="reset"||i!=null)){Le(e);return}s=s!=null?""+ae(s):"",i=i!=null?""+ae(i):s,N||i===e.value||(e.value=i),e.defaultValue=i}o=o??u,o=typeof o!="function"&&typeof o!="symbol"&&!!o,e.checked=N?e.checked:!!o,e.defaultChecked=!!o,M!=null&&typeof M!="function"&&typeof M!="symbol"&&typeof M!="boolean"&&(e.name=M),Le(e)}function Ee(e,i,s){i==="number"&&Qe(e.ownerDocument)===e||e.defaultValue===""+s||(e.defaultValue=""+s)}function Mn(e,i,s,o){if(e=e.options,i){i={};for(var u=0;u<s.length;u++)i["$"+s[u]]=!0;for(s=0;s<e.length;s++)u=i.hasOwnProperty("$"+e[s].value),e[s].selected!==u&&(e[s].selected=u),u&&o&&(e[s].defaultSelected=!0)}else{for(s=""+ae(s),i=null,u=0;u<e.length;u++){if(e[u].value===s){e[u].selected=!0,o&&(e[u].defaultSelected=!0);return}i!==null||e[u].disabled||(i=e[u])}i!==null&&(i.selected=!0)}}function ni(e,i,s){if(i!=null&&(i=""+ae(i),i!==e.value&&(e.value=i),s==null)){e.defaultValue!==i&&(e.defaultValue=i);return}e.defaultValue=s!=null?""+ae(s):""}function Di(e,i,s,o){if(i==null){if(o!=null){if(s!=null)throw Error(a(92));if(q(o)){if(1<o.length)throw Error(a(93));o=o[0]}s=o}s==null&&(s=""),i=s}s=ae(i),e.defaultValue=s,o=e.textContent,o===s&&o!==""&&o!==null&&(e.value=o),Le(e)}function ii(e,i){if(i){var s=e.firstChild;if(s&&s===e.lastChild&&s.nodeType===3){s.nodeValue=i;return}}e.textContent=i}var Ge=new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));function rn(e,i,s){var o=i.indexOf("--")===0;s==null||typeof s=="boolean"||s===""?o?e.setProperty(i,""):i==="float"?e.cssFloat="":e[i]="":o?e.setProperty(i,s):typeof s!="number"||s===0||Ge.has(i)?i==="float"?e.cssFloat=s:e[i]=(""+s).trim():e[i]=s+"px"}function Ui(e,i,s){if(i!=null&&typeof i!="object")throw Error(a(62));if(e=e.style,s!=null){for(var o in s)!s.hasOwnProperty(o)||i!=null&&i.hasOwnProperty(o)||(o.indexOf("--")===0?e.setProperty(o,""):o==="float"?e.cssFloat="":e[o]="");for(var u in i)o=i[u],i.hasOwnProperty(u)&&s[u]!==o&&rn(e,u,o)}else for(var p in i)i.hasOwnProperty(p)&&rn(e,p,i[p])}function Be(e){if(e.indexOf("-")===-1)return!1;switch(e){case"annotation-xml":case"color-profile":case"font-face":case"font-face-src":case"font-face-uri":case"font-face-format":case"font-face-name":case"missing-glyph":return!1;default:return!0}}var Gi=new Map([["acceptCharset","accept-charset"],["htmlFor","for"],["httpEquiv","http-equiv"],["crossOrigin","crossorigin"],["accentHeight","accent-height"],["alignmentBaseline","alignment-baseline"],["arabicForm","arabic-form"],["baselineShift","baseline-shift"],["capHeight","cap-height"],["clipPath","clip-path"],["clipRule","clip-rule"],["colorInterpolation","color-interpolation"],["colorInterpolationFilters","color-interpolation-filters"],["colorProfile","color-profile"],["colorRendering","color-rendering"],["dominantBaseline","dominant-baseline"],["enableBackground","enable-background"],["fillOpacity","fill-opacity"],["fillRule","fill-rule"],["floodColor","flood-color"],["floodOpacity","flood-opacity"],["fontFamily","font-family"],["fontSize","font-size"],["fontSizeAdjust","font-size-adjust"],["fontStretch","font-stretch"],["fontStyle","font-style"],["fontVariant","font-variant"],["fontWeight","font-weight"],["glyphName","glyph-name"],["glyphOrientationHorizontal","glyph-orientation-horizontal"],["glyphOrientationVertical","glyph-orientation-vertical"],["horizAdvX","horiz-adv-x"],["horizOriginX","horiz-origin-x"],["imageRendering","image-rendering"],["letterSpacing","letter-spacing"],["lightingColor","lighting-color"],["markerEnd","marker-end"],["markerMid","marker-mid"],["markerStart","marker-start"],["overlinePosition","overline-position"],["overlineThickness","overline-thickness"],["paintOrder","paint-order"],["panose-1","panose-1"],["pointerEvents","pointer-events"],["renderingIntent","rendering-intent"],["shapeRendering","shape-rendering"],["stopColor","stop-color"],["stopOpacity","stop-opacity"],["strikethroughPosition","strikethrough-position"],["strikethroughThickness","strikethrough-thickness"],["strokeDasharray","stroke-dasharray"],["strokeDashoffset","stroke-dashoffset"],["strokeLinecap","stroke-linecap"],["strokeLinejoin","stroke-linejoin"],["strokeMiterlimit","stroke-miterlimit"],["strokeOpacity","stroke-opacity"],["strokeWidth","stroke-width"],["textAnchor","text-anchor"],["textDecoration","text-decoration"],["textRendering","text-rendering"],["transformOrigin","transform-origin"],["underlinePosition","underline-position"],["underlineThickness","underline-thickness"],["unicodeBidi","unicode-bidi"],["unicodeRange","unicode-range"],["unitsPerEm","units-per-em"],["vAlphabetic","v-alphabetic"],["vHanging","v-hanging"],["vIdeographic","v-ideographic"],["vMathematical","v-mathematical"],["vectorEffect","vector-effect"],["vertAdvY","vert-adv-y"],["vertOriginX","vert-origin-x"],["vertOriginY","vert-origin-y"],["wordSpacing","word-spacing"],["writingMode","writing-mode"],["xmlnsXlink","xmlns:xlink"],["xHeight","x-height"]]),Fa=/^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;function Ms(e){return Fa.test(""+e)?"javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')":e}function aa(){}var Bu=null;function Fu(e){return e=e.target||e.srcElement||window,e.correspondingUseElement&&(e=e.correspondingUseElement),e.nodeType===3?e.parentNode:e}var ir=null,ar=null;function Jp(e){var i=za(e);if(i&&(e=i.stateNode)){var s=e[Cn]||null;t:switch(e=i.stateNode,i.type){case"input":if(qt(e,s.value,s.defaultValue,s.defaultValue,s.checked,s.defaultChecked,s.type,s.name),i=s.name,s.type==="radio"&&i!=null){for(s=e;s.parentNode;)s=s.parentNode;for(s=s.querySelectorAll('input[name="'+He(""+i)+'"][type="radio"]'),i=0;i<s.length;i++){var o=s[i];if(o!==e&&o.form===e.form){var u=o[Cn]||null;if(!u)throw Error(a(90));qt(o,u.value,u.defaultValue,u.defaultValue,u.checked,u.defaultChecked,u.type,u.name)}}for(i=0;i<s.length;i++)o=s[i],o.form===e.form&&sn(o)}break t;case"textarea":ni(e,s.value,s.defaultValue);break t;case"select":i=s.value,i!=null&&Mn(e,!!s.multiple,i,!1)}}}var Hu=!1;function $p(e,i,s){if(Hu)return e(i,s);Hu=!0;try{var o=e(i);return o}finally{if(Hu=!1,(ir!==null||ar!==null)&&(_c(),ir&&(i=ir,e=ar,ar=ir=null,Jp(i),e)))for(i=0;i<e.length;i++)Jp(e[i])}}function go(e,i){var s=e.stateNode;if(s===null)return null;var o=s[Cn]||null;if(o===null)return null;s=o[i];t:switch(i){case"onClick":case"onClickCapture":case"onDoubleClick":case"onDoubleClickCapture":case"onMouseDown":case"onMouseDownCapture":case"onMouseMove":case"onMouseMoveCapture":case"onMouseUp":case"onMouseUpCapture":case"onMouseEnter":(o=!o.disabled)||(e=e.type,o=!(e==="button"||e==="input"||e==="select"||e==="textarea")),e=!o;break t;default:e=!1}if(e)return null;if(s&&typeof s!="function")throw Error(a(231,i,typeof s));return s}var sa=!(typeof window>"u"||typeof window.document>"u"||typeof window.document.createElement>"u"),Gu=!1;if(sa)try{var _o={};Object.defineProperty(_o,"passive",{get:function(){Gu=!0}}),window.addEventListener("test",_o,_o),window.removeEventListener("test",_o,_o)}catch{Gu=!1}var Ha=null,Vu=null,Dl=null;function tm(){if(Dl)return Dl;var e,i=Vu,s=i.length,o,u="value"in Ha?Ha.value:Ha.textContent,p=u.length;for(e=0;e<s&&i[e]===u[e];e++);var M=s-e;for(o=1;o<=M&&i[s-o]===u[p-o];o++);return Dl=u.slice(e,1<o?1-o:void 0)}function Ul(e){var i=e.keyCode;return"charCode"in e?(e=e.charCode,e===0&&i===13&&(e=13)):e=i,e===10&&(e=13),32<=e||e===13?e:0}function Ll(){return!0}function em(){return!1}function Yn(e){function i(s,o,u,p,M){this._reactName=s,this._targetInst=u,this.type=o,this.nativeEvent=p,this.target=M,this.currentTarget=null;for(var N in e)e.hasOwnProperty(N)&&(s=e[N],this[N]=s?s(p):p[N]);return this.isDefaultPrevented=(p.defaultPrevented!=null?p.defaultPrevented:p.returnValue===!1)?Ll:em,this.isPropagationStopped=em,this}return _(i.prototype,{preventDefault:function(){this.defaultPrevented=!0;var s=this.nativeEvent;s&&(s.preventDefault?s.preventDefault():typeof s.returnValue!="unknown"&&(s.returnValue=!1),this.isDefaultPrevented=Ll)},stopPropagation:function(){var s=this.nativeEvent;s&&(s.stopPropagation?s.stopPropagation():typeof s.cancelBubble!="unknown"&&(s.cancelBubble=!0),this.isPropagationStopped=Ll)},persist:function(){},isPersistent:Ll}),i}var bs={eventPhase:0,bubbles:0,cancelable:0,timeStamp:function(e){return e.timeStamp||Date.now()},defaultPrevented:0,isTrusted:0},Ol=Yn(bs),vo=_({},bs,{view:0,detail:0}),ly=Yn(vo),ku,ju,xo,Pl=_({},vo,{screenX:0,screenY:0,clientX:0,clientY:0,pageX:0,pageY:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,getModifierState:Wu,button:0,buttons:0,relatedTarget:function(e){return e.relatedTarget===void 0?e.fromElement===e.srcElement?e.toElement:e.fromElement:e.relatedTarget},movementX:function(e){return"movementX"in e?e.movementX:(e!==xo&&(xo&&e.type==="mousemove"?(ku=e.screenX-xo.screenX,ju=e.screenY-xo.screenY):ju=ku=0,xo=e),ku)},movementY:function(e){return"movementY"in e?e.movementY:ju}}),nm=Yn(Pl),cy=_({},Pl,{dataTransfer:0}),uy=Yn(cy),fy=_({},vo,{relatedTarget:0}),Xu=Yn(fy),dy=_({},bs,{animationName:0,elapsedTime:0,pseudoElement:0}),hy=Yn(dy),py=_({},bs,{clipboardData:function(e){return"clipboardData"in e?e.clipboardData:window.clipboardData}}),my=Yn(py),gy=_({},bs,{data:0}),im=Yn(gy),_y={Esc:"Escape",Spacebar:" ",Left:"ArrowLeft",Up:"ArrowUp",Right:"ArrowRight",Down:"ArrowDown",Del:"Delete",Win:"OS",Menu:"ContextMenu",Apps:"ContextMenu",Scroll:"ScrollLock",MozPrintableKey:"Unidentified"},vy={8:"Backspace",9:"Tab",12:"Clear",13:"Enter",16:"Shift",17:"Control",18:"Alt",19:"Pause",20:"CapsLock",27:"Escape",32:" ",33:"PageUp",34:"PageDown",35:"End",36:"Home",37:"ArrowLeft",38:"ArrowUp",39:"ArrowRight",40:"ArrowDown",45:"Insert",46:"Delete",112:"F1",113:"F2",114:"F3",115:"F4",116:"F5",117:"F6",118:"F7",119:"F8",120:"F9",121:"F10",122:"F11",123:"F12",144:"NumLock",145:"ScrollLock",224:"Meta"},xy={Alt:"altKey",Control:"ctrlKey",Meta:"metaKey",Shift:"shiftKey"};function yy(e){var i=this.nativeEvent;return i.getModifierState?i.getModifierState(e):(e=xy[e])?!!i[e]:!1}function Wu(){return yy}var Sy=_({},vo,{key:function(e){if(e.key){var i=_y[e.key]||e.key;if(i!=="Unidentified")return i}return e.type==="keypress"?(e=Ul(e),e===13?"Enter":String.fromCharCode(e)):e.type==="keydown"||e.type==="keyup"?vy[e.keyCode]||"Unidentified":""},code:0,location:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,repeat:0,locale:0,getModifierState:Wu,charCode:function(e){return e.type==="keypress"?Ul(e):0},keyCode:function(e){return e.type==="keydown"||e.type==="keyup"?e.keyCode:0},which:function(e){return e.type==="keypress"?Ul(e):e.type==="keydown"||e.type==="keyup"?e.keyCode:0}}),My=Yn(Sy),by=_({},Pl,{pointerId:0,width:0,height:0,pressure:0,tangentialPressure:0,tiltX:0,tiltY:0,twist:0,pointerType:0,isPrimary:0}),am=Yn(by),Ey=_({},vo,{touches:0,targetTouches:0,changedTouches:0,altKey:0,metaKey:0,ctrlKey:0,shiftKey:0,getModifierState:Wu}),Ty=Yn(Ey),Ay=_({},bs,{propertyName:0,elapsedTime:0,pseudoElement:0}),wy=Yn(Ay),Ry=_({},Pl,{deltaX:function(e){return"deltaX"in e?e.deltaX:"wheelDeltaX"in e?-e.wheelDeltaX:0},deltaY:function(e){return"deltaY"in e?e.deltaY:"wheelDeltaY"in e?-e.wheelDeltaY:"wheelDelta"in e?-e.wheelDelta:0},deltaZ:0,deltaMode:0}),Cy=Yn(Ry),Ny=_({},bs,{newState:0,oldState:0}),Dy=Yn(Ny),Uy=[9,13,27,32],qu=sa&&"CompositionEvent"in window,yo=null;sa&&"documentMode"in document&&(yo=document.documentMode);var Ly=sa&&"TextEvent"in window&&!yo,sm=sa&&(!qu||yo&&8<yo&&11>=yo),rm=" ",om=!1;function lm(e,i){switch(e){case"keyup":return Uy.indexOf(i.keyCode)!==-1;case"keydown":return i.keyCode!==229;case"keypress":case"mousedown":case"focusout":return!0;default:return!1}}function cm(e){return e=e.detail,typeof e=="object"&&"data"in e?e.data:null}var sr=!1;function Oy(e,i){switch(e){case"compositionend":return cm(i);case"keypress":return i.which!==32?null:(om=!0,rm);case"textInput":return e=i.data,e===rm&&om?null:e;default:return null}}function Py(e,i){if(sr)return e==="compositionend"||!qu&&lm(e,i)?(e=tm(),Dl=Vu=Ha=null,sr=!1,e):null;switch(e){case"paste":return null;case"keypress":if(!(i.ctrlKey||i.altKey||i.metaKey)||i.ctrlKey&&i.altKey){if(i.char&&1<i.char.length)return i.char;if(i.which)return String.fromCharCode(i.which)}return null;case"compositionend":return sm&&i.locale!=="ko"?null:i.data;default:return null}}var Iy={color:!0,date:!0,datetime:!0,"datetime-local":!0,email:!0,month:!0,number:!0,password:!0,range:!0,search:!0,tel:!0,text:!0,time:!0,url:!0,week:!0};function um(e){var i=e&&e.nodeName&&e.nodeName.toLowerCase();return i==="input"?!!Iy[e.type]:i==="textarea"}function fm(e,i,s,o){ir?ar?ar.push(o):ar=[o]:ir=o,i=Ec(i,"onChange"),0<i.length&&(s=new Ol("onChange","change",null,s,o),e.push({event:s,listeners:i}))}var So=null,Mo=null;function zy(e){q0(e,0)}function Il(e){var i=Ss(e);if(sn(i))return e}function dm(e,i){if(e==="change")return i}var hm=!1;if(sa){var Yu;if(sa){var Zu="oninput"in document;if(!Zu){var pm=document.createElement("div");pm.setAttribute("oninput","return;"),Zu=typeof pm.oninput=="function"}Yu=Zu}else Yu=!1;hm=Yu&&(!document.documentMode||9<document.documentMode)}function mm(){So&&(So.detachEvent("onpropertychange",gm),Mo=So=null)}function gm(e){if(e.propertyName==="value"&&Il(Mo)){var i=[];fm(i,Mo,e,Fu(e)),$p(zy,i)}}function By(e,i,s){e==="focusin"?(mm(),So=i,Mo=s,So.attachEvent("onpropertychange",gm)):e==="focusout"&&mm()}function Fy(e){if(e==="selectionchange"||e==="keyup"||e==="keydown")return Il(Mo)}function Hy(e,i){if(e==="click")return Il(i)}function Gy(e,i){if(e==="input"||e==="change")return Il(i)}function Vy(e,i){return e===i&&(e!==0||1/e===1/i)||e!==e&&i!==i}var ai=typeof Object.is=="function"?Object.is:Vy;function bo(e,i){if(ai(e,i))return!0;if(typeof e!="object"||e===null||typeof i!="object"||i===null)return!1;var s=Object.keys(e),o=Object.keys(i);if(s.length!==o.length)return!1;for(o=0;o<s.length;o++){var u=s[o];if(!Ue.call(i,u)||!ai(e[u],i[u]))return!1}return!0}function _m(e){for(;e&&e.firstChild;)e=e.firstChild;return e}function vm(e,i){var s=_m(e);e=0;for(var o;s;){if(s.nodeType===3){if(o=e+s.textContent.length,e<=i&&o>=i)return{node:s,offset:i-e};e=o}t:{for(;s;){if(s.nextSibling){s=s.nextSibling;break t}s=s.parentNode}s=void 0}s=_m(s)}}function xm(e,i){return e&&i?e===i?!0:e&&e.nodeType===3?!1:i&&i.nodeType===3?xm(e,i.parentNode):"contains"in e?e.contains(i):e.compareDocumentPosition?!!(e.compareDocumentPosition(i)&16):!1:!1}function ym(e){e=e!=null&&e.ownerDocument!=null&&e.ownerDocument.defaultView!=null?e.ownerDocument.defaultView:window;for(var i=Qe(e.document);i instanceof e.HTMLIFrameElement;){try{var s=typeof i.contentWindow.location.href=="string"}catch{s=!1}if(s)e=i.contentWindow;else break;i=Qe(e.document)}return i}function Ku(e){var i=e&&e.nodeName&&e.nodeName.toLowerCase();return i&&(i==="input"&&(e.type==="text"||e.type==="search"||e.type==="tel"||e.type==="url"||e.type==="password")||i==="textarea"||e.contentEditable==="true")}var ky=sa&&"documentMode"in document&&11>=document.documentMode,rr=null,Qu=null,Eo=null,Ju=!1;function Sm(e,i,s){var o=s.window===s?s.document:s.nodeType===9?s:s.ownerDocument;Ju||rr==null||rr!==Qe(o)||(o=rr,"selectionStart"in o&&Ku(o)?o={start:o.selectionStart,end:o.selectionEnd}:(o=(o.ownerDocument&&o.ownerDocument.defaultView||window).getSelection(),o={anchorNode:o.anchorNode,anchorOffset:o.anchorOffset,focusNode:o.focusNode,focusOffset:o.focusOffset}),Eo&&bo(Eo,o)||(Eo=o,o=Ec(Qu,"onSelect"),0<o.length&&(i=new Ol("onSelect","select",null,i,s),e.push({event:i,listeners:o}),i.target=rr)))}function Es(e,i){var s={};return s[e.toLowerCase()]=i.toLowerCase(),s["Webkit"+e]="webkit"+i,s["Moz"+e]="moz"+i,s}var or={animationend:Es("Animation","AnimationEnd"),animationiteration:Es("Animation","AnimationIteration"),animationstart:Es("Animation","AnimationStart"),transitionrun:Es("Transition","TransitionRun"),transitionstart:Es("Transition","TransitionStart"),transitioncancel:Es("Transition","TransitionCancel"),transitionend:Es("Transition","TransitionEnd")},$u={},Mm={};sa&&(Mm=document.createElement("div").style,"AnimationEvent"in window||(delete or.animationend.animation,delete or.animationiteration.animation,delete or.animationstart.animation),"TransitionEvent"in window||delete or.transitionend.transition);function Ts(e){if($u[e])return $u[e];if(!or[e])return e;var i=or[e],s;for(s in i)if(i.hasOwnProperty(s)&&s in Mm)return $u[e]=i[s];return e}var bm=Ts("animationend"),Em=Ts("animationiteration"),Tm=Ts("animationstart"),jy=Ts("transitionrun"),Xy=Ts("transitionstart"),Wy=Ts("transitioncancel"),Am=Ts("transitionend"),wm=new Map,tf="abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");tf.push("scrollEnd");function Li(e,i){wm.set(e,i),Y(i,[e])}var zl=typeof reportError=="function"?reportError:function(e){if(typeof window=="object"&&typeof window.ErrorEvent=="function"){var i=new window.ErrorEvent("error",{bubbles:!0,cancelable:!0,message:typeof e=="object"&&e!==null&&typeof e.message=="string"?String(e.message):String(e),error:e});if(!window.dispatchEvent(i))return}else if(typeof process=="object"&&typeof process.emit=="function"){process.emit("uncaughtException",e);return}console.error(e)},gi=[],lr=0,ef=0;function Bl(){for(var e=lr,i=ef=lr=0;i<e;){var s=gi[i];gi[i++]=null;var o=gi[i];gi[i++]=null;var u=gi[i];gi[i++]=null;var p=gi[i];if(gi[i++]=null,o!==null&&u!==null){var M=o.pending;M===null?u.next=u:(u.next=M.next,M.next=u),o.pending=u}p!==0&&Rm(s,u,p)}}function Fl(e,i,s,o){gi[lr++]=e,gi[lr++]=i,gi[lr++]=s,gi[lr++]=o,ef|=o,e.lanes|=o,e=e.alternate,e!==null&&(e.lanes|=o)}function nf(e,i,s,o){return Fl(e,i,s,o),Hl(e)}function As(e,i){return Fl(e,null,null,i),Hl(e)}function Rm(e,i,s){e.lanes|=s;var o=e.alternate;o!==null&&(o.lanes|=s);for(var u=!1,p=e.return;p!==null;)p.childLanes|=s,o=p.alternate,o!==null&&(o.childLanes|=s),p.tag===22&&(e=p.stateNode,e===null||e._visibility&1||(u=!0)),e=p,p=p.return;return e.tag===3?(p=e.stateNode,u&&i!==null&&(u=31-Ft(s),e=p.hiddenUpdates,o=e[u],o===null?e[u]=[i]:o.push(i),i.lane=s|536870912),p):null}function Hl(e){if(50<Wo)throw Wo=0,dd=null,Error(a(185));for(var i=e.return;i!==null;)e=i,i=e.return;return e.tag===3?e.stateNode:null}var cr={};function qy(e,i,s,o){this.tag=e,this.key=s,this.sibling=this.child=this.return=this.stateNode=this.type=this.elementType=null,this.index=0,this.refCleanup=this.ref=null,this.pendingProps=i,this.dependencies=this.memoizedState=this.updateQueue=this.memoizedProps=null,this.mode=o,this.subtreeFlags=this.flags=0,this.deletions=null,this.childLanes=this.lanes=0,this.alternate=null}function si(e,i,s,o){return new qy(e,i,s,o)}function af(e){return e=e.prototype,!(!e||!e.isReactComponent)}function ra(e,i){var s=e.alternate;return s===null?(s=si(e.tag,i,e.key,e.mode),s.elementType=e.elementType,s.type=e.type,s.stateNode=e.stateNode,s.alternate=e,e.alternate=s):(s.pendingProps=i,s.type=e.type,s.flags=0,s.subtreeFlags=0,s.deletions=null),s.flags=e.flags&65011712,s.childLanes=e.childLanes,s.lanes=e.lanes,s.child=e.child,s.memoizedProps=e.memoizedProps,s.memoizedState=e.memoizedState,s.updateQueue=e.updateQueue,i=e.dependencies,s.dependencies=i===null?null:{lanes:i.lanes,firstContext:i.firstContext},s.sibling=e.sibling,s.index=e.index,s.ref=e.ref,s.refCleanup=e.refCleanup,s}function Cm(e,i){e.flags&=65011714;var s=e.alternate;return s===null?(e.childLanes=0,e.lanes=i,e.child=null,e.subtreeFlags=0,e.memoizedProps=null,e.memoizedState=null,e.updateQueue=null,e.dependencies=null,e.stateNode=null):(e.childLanes=s.childLanes,e.lanes=s.lanes,e.child=s.child,e.subtreeFlags=0,e.deletions=null,e.memoizedProps=s.memoizedProps,e.memoizedState=s.memoizedState,e.updateQueue=s.updateQueue,e.type=s.type,i=s.dependencies,e.dependencies=i===null?null:{lanes:i.lanes,firstContext:i.firstContext}),e}function Gl(e,i,s,o,u,p){var M=0;if(o=e,typeof e=="function")af(e)&&(M=1);else if(typeof e=="string")M=JS(e,s,Rt.current)?26:e==="html"||e==="head"||e==="body"?27:5;else t:switch(e){case z:return e=si(31,s,i,u),e.elementType=z,e.lanes=p,e;case A:return ws(s.children,u,p,i);case S:M=8,u|=24;break;case x:return e=si(12,s,i,u|2),e.elementType=x,e.lanes=p,e;case G:return e=si(13,s,i,u),e.elementType=G,e.lanes=p,e;case O:return e=si(19,s,i,u),e.elementType=O,e.lanes=p,e;default:if(typeof e=="object"&&e!==null)switch(e.$$typeof){case D:M=10;break t;case w:M=9;break t;case U:M=11;break t;case B:M=14;break t;case R:M=16,o=null;break t}M=29,s=Error(a(130,e===null?"null":typeof e,"")),o=null}return i=si(M,s,i,u),i.elementType=e,i.type=o,i.lanes=p,i}function ws(e,i,s,o){return e=si(7,e,o,i),e.lanes=s,e}function sf(e,i,s){return e=si(6,e,null,i),e.lanes=s,e}function Nm(e){var i=si(18,null,null,0);return i.stateNode=e,i}function rf(e,i,s){return i=si(4,e.children!==null?e.children:[],e.key,i),i.lanes=s,i.stateNode={containerInfo:e.containerInfo,pendingChildren:null,implementation:e.implementation},i}var Dm=new WeakMap;function _i(e,i){if(typeof e=="object"&&e!==null){var s=Dm.get(e);return s!==void 0?s:(i={value:e,source:i,stack:le(i)},Dm.set(e,i),i)}return{value:e,source:i,stack:le(i)}}var ur=[],fr=0,Vl=null,To=0,vi=[],xi=0,Ga=null,Vi=1,ki="";function oa(e,i){ur[fr++]=To,ur[fr++]=Vl,Vl=e,To=i}function Um(e,i,s){vi[xi++]=Vi,vi[xi++]=ki,vi[xi++]=Ga,Ga=e;var o=Vi;e=ki;var u=32-Ft(o)-1;o&=~(1<<u),s+=1;var p=32-Ft(i)+u;if(30<p){var M=u-u%5;p=(o&(1<<M)-1).toString(32),o>>=M,u-=M,Vi=1<<32-Ft(i)+u|s<<u|o,ki=p+e}else Vi=1<<p|s<<u|o,ki=e}function of(e){e.return!==null&&(oa(e,1),Um(e,1,0))}function lf(e){for(;e===Vl;)Vl=ur[--fr],ur[fr]=null,To=ur[--fr],ur[fr]=null;for(;e===Ga;)Ga=vi[--xi],vi[xi]=null,ki=vi[--xi],vi[xi]=null,Vi=vi[--xi],vi[xi]=null}function Lm(e,i){vi[xi++]=Vi,vi[xi++]=ki,vi[xi++]=Ga,Vi=i.id,ki=i.overflow,Ga=e}var Nn=null,en=null,Ce=!1,Va=null,yi=!1,cf=Error(a(519));function ka(e){var i=Error(a(418,1<arguments.length&&arguments[1]!==void 0&&arguments[1]?"text":"HTML",""));throw Ao(_i(i,e)),cf}function Om(e){var i=e.stateNode,s=e.type,o=e.memoizedProps;switch(i[dn]=e,i[Cn]=o,s){case"dialog":Ae("cancel",i),Ae("close",i);break;case"iframe":case"object":case"embed":Ae("load",i);break;case"video":case"audio":for(s=0;s<Yo.length;s++)Ae(Yo[s],i);break;case"source":Ae("error",i);break;case"img":case"image":case"link":Ae("error",i),Ae("load",i);break;case"details":Ae("toggle",i);break;case"input":Ae("invalid",i),zn(i,o.value,o.defaultValue,o.checked,o.defaultChecked,o.type,o.name,!0);break;case"select":Ae("invalid",i);break;case"textarea":Ae("invalid",i),Di(i,o.value,o.defaultValue,o.children)}s=o.children,typeof s!="string"&&typeof s!="number"&&typeof s!="bigint"||i.textContent===""+s||o.suppressHydrationWarning===!0||Q0(i.textContent,s)?(o.popover!=null&&(Ae("beforetoggle",i),Ae("toggle",i)),o.onScroll!=null&&Ae("scroll",i),o.onScrollEnd!=null&&Ae("scrollend",i),o.onClick!=null&&(i.onclick=aa),i=!0):i=!1,i||ka(e,!0)}function Pm(e){for(Nn=e.return;Nn;)switch(Nn.tag){case 5:case 31:case 13:yi=!1;return;case 27:case 3:yi=!0;return;default:Nn=Nn.return}}function dr(e){if(e!==Nn)return!1;if(!Ce)return Pm(e),Ce=!0,!1;var i=e.tag,s;if((s=i!==3&&i!==27)&&((s=i===5)&&(s=e.type,s=!(s!=="form"&&s!=="button")||wd(e.type,e.memoizedProps)),s=!s),s&&en&&ka(e),Pm(e),i===13){if(e=e.memoizedState,e=e!==null?e.dehydrated:null,!e)throw Error(a(317));en=r_(e)}else if(i===31){if(e=e.memoizedState,e=e!==null?e.dehydrated:null,!e)throw Error(a(317));en=r_(e)}else i===27?(i=en,is(e.type)?(e=Ud,Ud=null,en=e):en=i):en=Nn?Mi(e.stateNode.nextSibling):null;return!0}function Rs(){en=Nn=null,Ce=!1}function uf(){var e=Va;return e!==null&&(Jn===null?Jn=e:Jn.push.apply(Jn,e),Va=null),e}function Ao(e){Va===null?Va=[e]:Va.push(e)}var ff=I(null),Cs=null,la=null;function ja(e,i,s){Mt(ff,i._currentValue),i._currentValue=s}function ca(e){e._currentValue=ff.current,Q(ff)}function df(e,i,s){for(;e!==null;){var o=e.alternate;if((e.childLanes&i)!==i?(e.childLanes|=i,o!==null&&(o.childLanes|=i)):o!==null&&(o.childLanes&i)!==i&&(o.childLanes|=i),e===s)break;e=e.return}}function hf(e,i,s,o){var u=e.child;for(u!==null&&(u.return=e);u!==null;){var p=u.dependencies;if(p!==null){var M=u.child;p=p.firstContext;t:for(;p!==null;){var N=p;p=u;for(var H=0;H<i.length;H++)if(N.context===i[H]){p.lanes|=s,N=p.alternate,N!==null&&(N.lanes|=s),df(p.return,s,e),o||(M=null);break t}p=N.next}}else if(u.tag===18){if(M=u.return,M===null)throw Error(a(341));M.lanes|=s,p=M.alternate,p!==null&&(p.lanes|=s),df(M,s,e),M=null}else M=u.child;if(M!==null)M.return=u;else for(M=u;M!==null;){if(M===e){M=null;break}if(u=M.sibling,u!==null){u.return=M.return,M=u;break}M=M.return}u=M}}function hr(e,i,s,o){e=null;for(var u=i,p=!1;u!==null;){if(!p){if((u.flags&524288)!==0)p=!0;else if((u.flags&262144)!==0)break}if(u.tag===10){var M=u.alternate;if(M===null)throw Error(a(387));if(M=M.memoizedProps,M!==null){var N=u.type;ai(u.pendingProps.value,M.value)||(e!==null?e.push(N):e=[N])}}else if(u===bt.current){if(M=u.alternate,M===null)throw Error(a(387));M.memoizedState.memoizedState!==u.memoizedState.memoizedState&&(e!==null?e.push($o):e=[$o])}u=u.return}e!==null&&hf(i,e,s,o),i.flags|=262144}function kl(e){for(e=e.firstContext;e!==null;){if(!ai(e.context._currentValue,e.memoizedValue))return!0;e=e.next}return!1}function Ns(e){Cs=e,la=null,e=e.dependencies,e!==null&&(e.firstContext=null)}function Dn(e){return Im(Cs,e)}function jl(e,i){return Cs===null&&Ns(e),Im(e,i)}function Im(e,i){var s=i._currentValue;if(i={context:i,memoizedValue:s,next:null},la===null){if(e===null)throw Error(a(308));la=i,e.dependencies={lanes:0,firstContext:i},e.flags|=524288}else la=la.next=i;return s}var Yy=typeof AbortController<"u"?AbortController:function(){var e=[],i=this.signal={aborted:!1,addEventListener:function(s,o){e.push(o)}};this.abort=function(){i.aborted=!0,e.forEach(function(s){return s()})}},Zy=r.unstable_scheduleCallback,Ky=r.unstable_NormalPriority,gn={$$typeof:D,Consumer:null,Provider:null,_currentValue:null,_currentValue2:null,_threadCount:0};function pf(){return{controller:new Yy,data:new Map,refCount:0}}function wo(e){e.refCount--,e.refCount===0&&Zy(Ky,function(){e.controller.abort()})}var Ro=null,mf=0,pr=0,mr=null;function Qy(e,i){if(Ro===null){var s=Ro=[];mf=0,pr=vd(),mr={status:"pending",value:void 0,then:function(o){s.push(o)}}}return mf++,i.then(zm,zm),i}function zm(){if(--mf===0&&Ro!==null){mr!==null&&(mr.status="fulfilled");var e=Ro;Ro=null,pr=0,mr=null;for(var i=0;i<e.length;i++)(0,e[i])()}}function Jy(e,i){var s=[],o={status:"pending",value:null,reason:null,then:function(u){s.push(u)}};return e.then(function(){o.status="fulfilled",o.value=i;for(var u=0;u<s.length;u++)(0,s[u])(i)},function(u){for(o.status="rejected",o.reason=u,u=0;u<s.length;u++)(0,s[u])(void 0)}),o}var Bm=P.S;P.S=function(e,i){S0=Dt(),typeof i=="object"&&i!==null&&typeof i.then=="function"&&Qy(e,i),Bm!==null&&Bm(e,i)};var Ds=I(null);function gf(){var e=Ds.current;return e!==null?e:Je.pooledCache}function Xl(e,i){i===null?Mt(Ds,Ds.current):Mt(Ds,i.pool)}function Fm(){var e=gf();return e===null?null:{parent:gn._currentValue,pool:e}}var gr=Error(a(460)),_f=Error(a(474)),Wl=Error(a(542)),ql={then:function(){}};function Hm(e){return e=e.status,e==="fulfilled"||e==="rejected"}function Gm(e,i,s){switch(s=e[s],s===void 0?e.push(i):s!==i&&(i.then(aa,aa),i=s),i.status){case"fulfilled":return i.value;case"rejected":throw e=i.reason,km(e),e;default:if(typeof i.status=="string")i.then(aa,aa);else{if(e=Je,e!==null&&100<e.shellSuspendCounter)throw Error(a(482));e=i,e.status="pending",e.then(function(o){if(i.status==="pending"){var u=i;u.status="fulfilled",u.value=o}},function(o){if(i.status==="pending"){var u=i;u.status="rejected",u.reason=o}})}switch(i.status){case"fulfilled":return i.value;case"rejected":throw e=i.reason,km(e),e}throw Ls=i,gr}}function Us(e){try{var i=e._init;return i(e._payload)}catch(s){throw s!==null&&typeof s=="object"&&typeof s.then=="function"?(Ls=s,gr):s}}var Ls=null;function Vm(){if(Ls===null)throw Error(a(459));var e=Ls;return Ls=null,e}function km(e){if(e===gr||e===Wl)throw Error(a(483))}var _r=null,Co=0;function Yl(e){var i=Co;return Co+=1,_r===null&&(_r=[]),Gm(_r,e,i)}function No(e,i){i=i.props.ref,e.ref=i!==void 0?i:null}function Zl(e,i){throw i.$$typeof===v?Error(a(525)):(e=Object.prototype.toString.call(i),Error(a(31,e==="[object Object]"?"object with keys {"+Object.keys(i).join(", ")+"}":e)))}function jm(e){function i(Z,X){if(e){var et=Z.deletions;et===null?(Z.deletions=[X],Z.flags|=16):et.push(X)}}function s(Z,X){if(!e)return null;for(;X!==null;)i(Z,X),X=X.sibling;return null}function o(Z){for(var X=new Map;Z!==null;)Z.key!==null?X.set(Z.key,Z):X.set(Z.index,Z),Z=Z.sibling;return X}function u(Z,X){return Z=ra(Z,X),Z.index=0,Z.sibling=null,Z}function p(Z,X,et){return Z.index=et,e?(et=Z.alternate,et!==null?(et=et.index,et<X?(Z.flags|=67108866,X):et):(Z.flags|=67108866,X)):(Z.flags|=1048576,X)}function M(Z){return e&&Z.alternate===null&&(Z.flags|=67108866),Z}function N(Z,X,et,vt){return X===null||X.tag!==6?(X=sf(et,Z.mode,vt),X.return=Z,X):(X=u(X,et),X.return=Z,X)}function H(Z,X,et,vt){var se=et.type;return se===A?mt(Z,X,et.props.children,vt,et.key):X!==null&&(X.elementType===se||typeof se=="object"&&se!==null&&se.$$typeof===R&&Us(se)===X.type)?(X=u(X,et.props),No(X,et),X.return=Z,X):(X=Gl(et.type,et.key,et.props,null,Z.mode,vt),No(X,et),X.return=Z,X)}function nt(Z,X,et,vt){return X===null||X.tag!==4||X.stateNode.containerInfo!==et.containerInfo||X.stateNode.implementation!==et.implementation?(X=rf(et,Z.mode,vt),X.return=Z,X):(X=u(X,et.children||[]),X.return=Z,X)}function mt(Z,X,et,vt,se){return X===null||X.tag!==7?(X=ws(et,Z.mode,vt,se),X.return=Z,X):(X=u(X,et),X.return=Z,X)}function St(Z,X,et){if(typeof X=="string"&&X!==""||typeof X=="number"||typeof X=="bigint")return X=sf(""+X,Z.mode,et),X.return=Z,X;if(typeof X=="object"&&X!==null){switch(X.$$typeof){case y:return et=Gl(X.type,X.key,X.props,null,Z.mode,et),No(et,X),et.return=Z,et;case E:return X=rf(X,Z.mode,et),X.return=Z,X;case R:return X=Us(X),St(Z,X,et)}if(q(X)||$(X))return X=ws(X,Z.mode,et,null),X.return=Z,X;if(typeof X.then=="function")return St(Z,Yl(X),et);if(X.$$typeof===D)return St(Z,jl(Z,X),et);Zl(Z,X)}return null}function lt(Z,X,et,vt){var se=X!==null?X.key:null;if(typeof et=="string"&&et!==""||typeof et=="number"||typeof et=="bigint")return se!==null?null:N(Z,X,""+et,vt);if(typeof et=="object"&&et!==null){switch(et.$$typeof){case y:return et.key===se?H(Z,X,et,vt):null;case E:return et.key===se?nt(Z,X,et,vt):null;case R:return et=Us(et),lt(Z,X,et,vt)}if(q(et)||$(et))return se!==null?null:mt(Z,X,et,vt,null);if(typeof et.then=="function")return lt(Z,X,Yl(et),vt);if(et.$$typeof===D)return lt(Z,X,jl(Z,et),vt);Zl(Z,et)}return null}function ft(Z,X,et,vt,se){if(typeof vt=="string"&&vt!==""||typeof vt=="number"||typeof vt=="bigint")return Z=Z.get(et)||null,N(X,Z,""+vt,se);if(typeof vt=="object"&&vt!==null){switch(vt.$$typeof){case y:return Z=Z.get(vt.key===null?et:vt.key)||null,H(X,Z,vt,se);case E:return Z=Z.get(vt.key===null?et:vt.key)||null,nt(X,Z,vt,se);case R:return vt=Us(vt),ft(Z,X,et,vt,se)}if(q(vt)||$(vt))return Z=Z.get(et)||null,mt(X,Z,vt,se,null);if(typeof vt.then=="function")return ft(Z,X,et,Yl(vt),se);if(vt.$$typeof===D)return ft(Z,X,et,jl(X,vt),se);Zl(X,vt)}return null}function Qt(Z,X,et,vt){for(var se=null,Oe=null,$t=X,ve=X=0,Re=null;$t!==null&&ve<et.length;ve++){$t.index>ve?(Re=$t,$t=null):Re=$t.sibling;var Pe=lt(Z,$t,et[ve],vt);if(Pe===null){$t===null&&($t=Re);break}e&&$t&&Pe.alternate===null&&i(Z,$t),X=p(Pe,X,ve),Oe===null?se=Pe:Oe.sibling=Pe,Oe=Pe,$t=Re}if(ve===et.length)return s(Z,$t),Ce&&oa(Z,ve),se;if($t===null){for(;ve<et.length;ve++)$t=St(Z,et[ve],vt),$t!==null&&(X=p($t,X,ve),Oe===null?se=$t:Oe.sibling=$t,Oe=$t);return Ce&&oa(Z,ve),se}for($t=o($t);ve<et.length;ve++)Re=ft($t,Z,ve,et[ve],vt),Re!==null&&(e&&Re.alternate!==null&&$t.delete(Re.key===null?ve:Re.key),X=p(Re,X,ve),Oe===null?se=Re:Oe.sibling=Re,Oe=Re);return e&&$t.forEach(function(ls){return i(Z,ls)}),Ce&&oa(Z,ve),se}function oe(Z,X,et,vt){if(et==null)throw Error(a(151));for(var se=null,Oe=null,$t=X,ve=X=0,Re=null,Pe=et.next();$t!==null&&!Pe.done;ve++,Pe=et.next()){$t.index>ve?(Re=$t,$t=null):Re=$t.sibling;var ls=lt(Z,$t,Pe.value,vt);if(ls===null){$t===null&&($t=Re);break}e&&$t&&ls.alternate===null&&i(Z,$t),X=p(ls,X,ve),Oe===null?se=ls:Oe.sibling=ls,Oe=ls,$t=Re}if(Pe.done)return s(Z,$t),Ce&&oa(Z,ve),se;if($t===null){for(;!Pe.done;ve++,Pe=et.next())Pe=St(Z,Pe.value,vt),Pe!==null&&(X=p(Pe,X,ve),Oe===null?se=Pe:Oe.sibling=Pe,Oe=Pe);return Ce&&oa(Z,ve),se}for($t=o($t);!Pe.done;ve++,Pe=et.next())Pe=ft($t,Z,ve,Pe.value,vt),Pe!==null&&(e&&Pe.alternate!==null&&$t.delete(Pe.key===null?ve:Pe.key),X=p(Pe,X,ve),Oe===null?se=Pe:Oe.sibling=Pe,Oe=Pe);return e&&$t.forEach(function(cM){return i(Z,cM)}),Ce&&oa(Z,ve),se}function Ke(Z,X,et,vt){if(typeof et=="object"&&et!==null&&et.type===A&&et.key===null&&(et=et.props.children),typeof et=="object"&&et!==null){switch(et.$$typeof){case y:t:{for(var se=et.key;X!==null;){if(X.key===se){if(se=et.type,se===A){if(X.tag===7){s(Z,X.sibling),vt=u(X,et.props.children),vt.return=Z,Z=vt;break t}}else if(X.elementType===se||typeof se=="object"&&se!==null&&se.$$typeof===R&&Us(se)===X.type){s(Z,X.sibling),vt=u(X,et.props),No(vt,et),vt.return=Z,Z=vt;break t}s(Z,X);break}else i(Z,X);X=X.sibling}et.type===A?(vt=ws(et.props.children,Z.mode,vt,et.key),vt.return=Z,Z=vt):(vt=Gl(et.type,et.key,et.props,null,Z.mode,vt),No(vt,et),vt.return=Z,Z=vt)}return M(Z);case E:t:{for(se=et.key;X!==null;){if(X.key===se)if(X.tag===4&&X.stateNode.containerInfo===et.containerInfo&&X.stateNode.implementation===et.implementation){s(Z,X.sibling),vt=u(X,et.children||[]),vt.return=Z,Z=vt;break t}else{s(Z,X);break}else i(Z,X);X=X.sibling}vt=rf(et,Z.mode,vt),vt.return=Z,Z=vt}return M(Z);case R:return et=Us(et),Ke(Z,X,et,vt)}if(q(et))return Qt(Z,X,et,vt);if($(et)){if(se=$(et),typeof se!="function")throw Error(a(150));return et=se.call(et),oe(Z,X,et,vt)}if(typeof et.then=="function")return Ke(Z,X,Yl(et),vt);if(et.$$typeof===D)return Ke(Z,X,jl(Z,et),vt);Zl(Z,et)}return typeof et=="string"&&et!==""||typeof et=="number"||typeof et=="bigint"?(et=""+et,X!==null&&X.tag===6?(s(Z,X.sibling),vt=u(X,et),vt.return=Z,Z=vt):(s(Z,X),vt=sf(et,Z.mode,vt),vt.return=Z,Z=vt),M(Z)):s(Z,X)}return function(Z,X,et,vt){try{Co=0;var se=Ke(Z,X,et,vt);return _r=null,se}catch($t){if($t===gr||$t===Wl)throw $t;var Oe=si(29,$t,null,Z.mode);return Oe.lanes=vt,Oe.return=Z,Oe}finally{}}}var Os=jm(!0),Xm=jm(!1),Xa=!1;function vf(e){e.updateQueue={baseState:e.memoizedState,firstBaseUpdate:null,lastBaseUpdate:null,shared:{pending:null,lanes:0,hiddenCallbacks:null},callbacks:null}}function xf(e,i){e=e.updateQueue,i.updateQueue===e&&(i.updateQueue={baseState:e.baseState,firstBaseUpdate:e.firstBaseUpdate,lastBaseUpdate:e.lastBaseUpdate,shared:e.shared,callbacks:null})}function Wa(e){return{lane:e,tag:0,payload:null,callback:null,next:null}}function qa(e,i,s){var o=e.updateQueue;if(o===null)return null;if(o=o.shared,(ze&2)!==0){var u=o.pending;return u===null?i.next=i:(i.next=u.next,u.next=i),o.pending=i,i=Hl(e),Rm(e,null,s),i}return Fl(e,o,i,s),Hl(e)}function Do(e,i,s){if(i=i.updateQueue,i!==null&&(i=i.shared,(s&4194048)!==0)){var o=i.lanes;o&=e.pendingLanes,s|=o,i.lanes=s,mi(e,s)}}function yf(e,i){var s=e.updateQueue,o=e.alternate;if(o!==null&&(o=o.updateQueue,s===o)){var u=null,p=null;if(s=s.firstBaseUpdate,s!==null){do{var M={lane:s.lane,tag:s.tag,payload:s.payload,callback:null,next:null};p===null?u=p=M:p=p.next=M,s=s.next}while(s!==null);p===null?u=p=i:p=p.next=i}else u=p=i;s={baseState:o.baseState,firstBaseUpdate:u,lastBaseUpdate:p,shared:o.shared,callbacks:o.callbacks},e.updateQueue=s;return}e=s.lastBaseUpdate,e===null?s.firstBaseUpdate=i:e.next=i,s.lastBaseUpdate=i}var Sf=!1;function Uo(){if(Sf){var e=mr;if(e!==null)throw e}}function Lo(e,i,s,o){Sf=!1;var u=e.updateQueue;Xa=!1;var p=u.firstBaseUpdate,M=u.lastBaseUpdate,N=u.shared.pending;if(N!==null){u.shared.pending=null;var H=N,nt=H.next;H.next=null,M===null?p=nt:M.next=nt,M=H;var mt=e.alternate;mt!==null&&(mt=mt.updateQueue,N=mt.lastBaseUpdate,N!==M&&(N===null?mt.firstBaseUpdate=nt:N.next=nt,mt.lastBaseUpdate=H))}if(p!==null){var St=u.baseState;M=0,mt=nt=H=null,N=p;do{var lt=N.lane&-536870913,ft=lt!==N.lane;if(ft?(we&lt)===lt:(o&lt)===lt){lt!==0&&lt===pr&&(Sf=!0),mt!==null&&(mt=mt.next={lane:0,tag:N.tag,payload:N.payload,callback:null,next:null});t:{var Qt=e,oe=N;lt=i;var Ke=s;switch(oe.tag){case 1:if(Qt=oe.payload,typeof Qt=="function"){St=Qt.call(Ke,St,lt);break t}St=Qt;break t;case 3:Qt.flags=Qt.flags&-65537|128;case 0:if(Qt=oe.payload,lt=typeof Qt=="function"?Qt.call(Ke,St,lt):Qt,lt==null)break t;St=_({},St,lt);break t;case 2:Xa=!0}}lt=N.callback,lt!==null&&(e.flags|=64,ft&&(e.flags|=8192),ft=u.callbacks,ft===null?u.callbacks=[lt]:ft.push(lt))}else ft={lane:lt,tag:N.tag,payload:N.payload,callback:N.callback,next:null},mt===null?(nt=mt=ft,H=St):mt=mt.next=ft,M|=lt;if(N=N.next,N===null){if(N=u.shared.pending,N===null)break;ft=N,N=ft.next,ft.next=null,u.lastBaseUpdate=ft,u.shared.pending=null}}while(!0);mt===null&&(H=St),u.baseState=H,u.firstBaseUpdate=nt,u.lastBaseUpdate=mt,p===null&&(u.shared.lanes=0),Ja|=M,e.lanes=M,e.memoizedState=St}}function Wm(e,i){if(typeof e!="function")throw Error(a(191,e));e.call(i)}function qm(e,i){var s=e.callbacks;if(s!==null)for(e.callbacks=null,e=0;e<s.length;e++)Wm(s[e],i)}var vr=I(null),Kl=I(0);function Ym(e,i){e=va,Mt(Kl,e),Mt(vr,i),va=e|i.baseLanes}function Mf(){Mt(Kl,va),Mt(vr,vr.current)}function bf(){va=Kl.current,Q(vr),Q(Kl)}var ri=I(null),Si=null;function Ya(e){var i=e.alternate;Mt(hn,hn.current&1),Mt(ri,e),Si===null&&(i===null||vr.current!==null||i.memoizedState!==null)&&(Si=e)}function Ef(e){Mt(hn,hn.current),Mt(ri,e),Si===null&&(Si=e)}function Zm(e){e.tag===22?(Mt(hn,hn.current),Mt(ri,e),Si===null&&(Si=e)):Za()}function Za(){Mt(hn,hn.current),Mt(ri,ri.current)}function oi(e){Q(ri),Si===e&&(Si=null),Q(hn)}var hn=I(0);function Ql(e){for(var i=e;i!==null;){if(i.tag===13){var s=i.memoizedState;if(s!==null&&(s=s.dehydrated,s===null||Nd(s)||Dd(s)))return i}else if(i.tag===19&&(i.memoizedProps.revealOrder==="forwards"||i.memoizedProps.revealOrder==="backwards"||i.memoizedProps.revealOrder==="unstable_legacy-backwards"||i.memoizedProps.revealOrder==="together")){if((i.flags&128)!==0)return i}else if(i.child!==null){i.child.return=i,i=i.child;continue}if(i===e)break;for(;i.sibling===null;){if(i.return===null||i.return===e)return null;i=i.return}i.sibling.return=i.return,i=i.sibling}return null}var ua=0,ge=null,Ye=null,_n=null,Jl=!1,xr=!1,Ps=!1,$l=0,Oo=0,yr=null,$y=0;function cn(){throw Error(a(321))}function Tf(e,i){if(i===null)return!1;for(var s=0;s<i.length&&s<e.length;s++)if(!ai(e[s],i[s]))return!1;return!0}function Af(e,i,s,o,u,p){return ua=p,ge=i,i.memoizedState=null,i.updateQueue=null,i.lanes=0,P.H=e===null||e.memoizedState===null?Ug:Gf,Ps=!1,p=s(o,u),Ps=!1,xr&&(p=Qm(i,s,o,u)),Km(e),p}function Km(e){P.H=zo;var i=Ye!==null&&Ye.next!==null;if(ua=0,_n=Ye=ge=null,Jl=!1,Oo=0,yr=null,i)throw Error(a(300));e===null||vn||(e=e.dependencies,e!==null&&kl(e)&&(vn=!0))}function Qm(e,i,s,o){ge=e;var u=0;do{if(xr&&(yr=null),Oo=0,xr=!1,25<=u)throw Error(a(301));if(u+=1,_n=Ye=null,e.updateQueue!=null){var p=e.updateQueue;p.lastEffect=null,p.events=null,p.stores=null,p.memoCache!=null&&(p.memoCache.index=0)}P.H=Lg,p=i(s,o)}while(xr);return p}function tS(){var e=P.H,i=e.useState()[0];return i=typeof i.then=="function"?Po(i):i,e=e.useState()[0],(Ye!==null?Ye.memoizedState:null)!==e&&(ge.flags|=1024),i}function wf(){var e=$l!==0;return $l=0,e}function Rf(e,i,s){i.updateQueue=e.updateQueue,i.flags&=-2053,e.lanes&=~s}function Cf(e){if(Jl){for(e=e.memoizedState;e!==null;){var i=e.queue;i!==null&&(i.pending=null),e=e.next}Jl=!1}ua=0,_n=Ye=ge=null,xr=!1,Oo=$l=0,yr=null}function jn(){var e={memoizedState:null,baseState:null,baseQueue:null,queue:null,next:null};return _n===null?ge.memoizedState=_n=e:_n=_n.next=e,_n}function pn(){if(Ye===null){var e=ge.alternate;e=e!==null?e.memoizedState:null}else e=Ye.next;var i=_n===null?ge.memoizedState:_n.next;if(i!==null)_n=i,Ye=e;else{if(e===null)throw ge.alternate===null?Error(a(467)):Error(a(310));Ye=e,e={memoizedState:Ye.memoizedState,baseState:Ye.baseState,baseQueue:Ye.baseQueue,queue:Ye.queue,next:null},_n===null?ge.memoizedState=_n=e:_n=_n.next=e}return _n}function tc(){return{lastEffect:null,events:null,stores:null,memoCache:null}}function Po(e){var i=Oo;return Oo+=1,yr===null&&(yr=[]),e=Gm(yr,e,i),i=ge,(_n===null?i.memoizedState:_n.next)===null&&(i=i.alternate,P.H=i===null||i.memoizedState===null?Ug:Gf),e}function ec(e){if(e!==null&&typeof e=="object"){if(typeof e.then=="function")return Po(e);if(e.$$typeof===D)return Dn(e)}throw Error(a(438,String(e)))}function Nf(e){var i=null,s=ge.updateQueue;if(s!==null&&(i=s.memoCache),i==null){var o=ge.alternate;o!==null&&(o=o.updateQueue,o!==null&&(o=o.memoCache,o!=null&&(i={data:o.data.map(function(u){return u.slice()}),index:0})))}if(i==null&&(i={data:[],index:0}),s===null&&(s=tc(),ge.updateQueue=s),s.memoCache=i,s=i.data[i.index],s===void 0)for(s=i.data[i.index]=Array(e),o=0;o<e;o++)s[o]=K;return i.index++,s}function fa(e,i){return typeof i=="function"?i(e):i}function nc(e){var i=pn();return Df(i,Ye,e)}function Df(e,i,s){var o=e.queue;if(o===null)throw Error(a(311));o.lastRenderedReducer=s;var u=e.baseQueue,p=o.pending;if(p!==null){if(u!==null){var M=u.next;u.next=p.next,p.next=M}i.baseQueue=u=p,o.pending=null}if(p=e.baseState,u===null)e.memoizedState=p;else{i=u.next;var N=M=null,H=null,nt=i,mt=!1;do{var St=nt.lane&-536870913;if(St!==nt.lane?(we&St)===St:(ua&St)===St){var lt=nt.revertLane;if(lt===0)H!==null&&(H=H.next={lane:0,revertLane:0,gesture:null,action:nt.action,hasEagerState:nt.hasEagerState,eagerState:nt.eagerState,next:null}),St===pr&&(mt=!0);else if((ua&lt)===lt){nt=nt.next,lt===pr&&(mt=!0);continue}else St={lane:0,revertLane:nt.revertLane,gesture:null,action:nt.action,hasEagerState:nt.hasEagerState,eagerState:nt.eagerState,next:null},H===null?(N=H=St,M=p):H=H.next=St,ge.lanes|=lt,Ja|=lt;St=nt.action,Ps&&s(p,St),p=nt.hasEagerState?nt.eagerState:s(p,St)}else lt={lane:St,revertLane:nt.revertLane,gesture:nt.gesture,action:nt.action,hasEagerState:nt.hasEagerState,eagerState:nt.eagerState,next:null},H===null?(N=H=lt,M=p):H=H.next=lt,ge.lanes|=St,Ja|=St;nt=nt.next}while(nt!==null&&nt!==i);if(H===null?M=p:H.next=N,!ai(p,e.memoizedState)&&(vn=!0,mt&&(s=mr,s!==null)))throw s;e.memoizedState=p,e.baseState=M,e.baseQueue=H,o.lastRenderedState=p}return u===null&&(o.lanes=0),[e.memoizedState,o.dispatch]}function Uf(e){var i=pn(),s=i.queue;if(s===null)throw Error(a(311));s.lastRenderedReducer=e;var o=s.dispatch,u=s.pending,p=i.memoizedState;if(u!==null){s.pending=null;var M=u=u.next;do p=e(p,M.action),M=M.next;while(M!==u);ai(p,i.memoizedState)||(vn=!0),i.memoizedState=p,i.baseQueue===null&&(i.baseState=p),s.lastRenderedState=p}return[p,o]}function Jm(e,i,s){var o=ge,u=pn(),p=Ce;if(p){if(s===void 0)throw Error(a(407));s=s()}else s=i();var M=!ai((Ye||u).memoizedState,s);if(M&&(u.memoizedState=s,vn=!0),u=u.queue,Pf(eg.bind(null,o,u,e),[e]),u.getSnapshot!==i||M||_n!==null&&_n.memoizedState.tag&1){if(o.flags|=2048,Sr(9,{destroy:void 0},tg.bind(null,o,u,s,i),null),Je===null)throw Error(a(349));p||(ua&127)!==0||$m(o,i,s)}return s}function $m(e,i,s){e.flags|=16384,e={getSnapshot:i,value:s},i=ge.updateQueue,i===null?(i=tc(),ge.updateQueue=i,i.stores=[e]):(s=i.stores,s===null?i.stores=[e]:s.push(e))}function tg(e,i,s,o){i.value=s,i.getSnapshot=o,ng(i)&&ig(e)}function eg(e,i,s){return s(function(){ng(i)&&ig(e)})}function ng(e){var i=e.getSnapshot;e=e.value;try{var s=i();return!ai(e,s)}catch{return!0}}function ig(e){var i=As(e,2);i!==null&&$n(i,e,2)}function Lf(e){var i=jn();if(typeof e=="function"){var s=e;if(e=s(),Ps){Bt(!0);try{s()}finally{Bt(!1)}}}return i.memoizedState=i.baseState=e,i.queue={pending:null,lanes:0,dispatch:null,lastRenderedReducer:fa,lastRenderedState:e},i}function ag(e,i,s,o){return e.baseState=s,Df(e,Ye,typeof o=="function"?o:fa)}function eS(e,i,s,o,u){if(sc(e))throw Error(a(485));if(e=i.action,e!==null){var p={payload:u,action:e,next:null,isTransition:!0,status:"pending",value:null,reason:null,listeners:[],then:function(M){p.listeners.push(M)}};P.T!==null?s(!0):p.isTransition=!1,o(p),s=i.pending,s===null?(p.next=i.pending=p,sg(i,p)):(p.next=s.next,i.pending=s.next=p)}}function sg(e,i){var s=i.action,o=i.payload,u=e.state;if(i.isTransition){var p=P.T,M={};P.T=M;try{var N=s(u,o),H=P.S;H!==null&&H(M,N),rg(e,i,N)}catch(nt){Of(e,i,nt)}finally{p!==null&&M.types!==null&&(p.types=M.types),P.T=p}}else try{p=s(u,o),rg(e,i,p)}catch(nt){Of(e,i,nt)}}function rg(e,i,s){s!==null&&typeof s=="object"&&typeof s.then=="function"?s.then(function(o){og(e,i,o)},function(o){return Of(e,i,o)}):og(e,i,s)}function og(e,i,s){i.status="fulfilled",i.value=s,lg(i),e.state=s,i=e.pending,i!==null&&(s=i.next,s===i?e.pending=null:(s=s.next,i.next=s,sg(e,s)))}function Of(e,i,s){var o=e.pending;if(e.pending=null,o!==null){o=o.next;do i.status="rejected",i.reason=s,lg(i),i=i.next;while(i!==o)}e.action=null}function lg(e){e=e.listeners;for(var i=0;i<e.length;i++)(0,e[i])()}function cg(e,i){return i}function ug(e,i){if(Ce){var s=Je.formState;if(s!==null){t:{var o=ge;if(Ce){if(en){e:{for(var u=en,p=yi;u.nodeType!==8;){if(!p){u=null;break e}if(u=Mi(u.nextSibling),u===null){u=null;break e}}p=u.data,u=p==="F!"||p==="F"?u:null}if(u){en=Mi(u.nextSibling),o=u.data==="F!";break t}}ka(o)}o=!1}o&&(i=s[0])}}return s=jn(),s.memoizedState=s.baseState=i,o={pending:null,lanes:0,dispatch:null,lastRenderedReducer:cg,lastRenderedState:i},s.queue=o,s=Cg.bind(null,ge,o),o.dispatch=s,o=Lf(!1),p=Hf.bind(null,ge,!1,o.queue),o=jn(),u={state:i,dispatch:null,action:e,pending:null},o.queue=u,s=eS.bind(null,ge,u,p,s),u.dispatch=s,o.memoizedState=e,[i,s,!1]}function fg(e){var i=pn();return dg(i,Ye,e)}function dg(e,i,s){if(i=Df(e,i,cg)[0],e=nc(fa)[0],typeof i=="object"&&i!==null&&typeof i.then=="function")try{var o=Po(i)}catch(M){throw M===gr?Wl:M}else o=i;i=pn();var u=i.queue,p=u.dispatch;return s!==i.memoizedState&&(ge.flags|=2048,Sr(9,{destroy:void 0},nS.bind(null,u,s),null)),[o,p,e]}function nS(e,i){e.action=i}function hg(e){var i=pn(),s=Ye;if(s!==null)return dg(i,s,e);pn(),i=i.memoizedState,s=pn();var o=s.queue.dispatch;return s.memoizedState=e,[i,o,!1]}function Sr(e,i,s,o){return e={tag:e,create:s,deps:o,inst:i,next:null},i=ge.updateQueue,i===null&&(i=tc(),ge.updateQueue=i),s=i.lastEffect,s===null?i.lastEffect=e.next=e:(o=s.next,s.next=e,e.next=o,i.lastEffect=e),e}function pg(){return pn().memoizedState}function ic(e,i,s,o){var u=jn();ge.flags|=e,u.memoizedState=Sr(1|i,{destroy:void 0},s,o===void 0?null:o)}function ac(e,i,s,o){var u=pn();o=o===void 0?null:o;var p=u.memoizedState.inst;Ye!==null&&o!==null&&Tf(o,Ye.memoizedState.deps)?u.memoizedState=Sr(i,p,s,o):(ge.flags|=e,u.memoizedState=Sr(1|i,p,s,o))}function mg(e,i){ic(8390656,8,e,i)}function Pf(e,i){ac(2048,8,e,i)}function iS(e){ge.flags|=4;var i=ge.updateQueue;if(i===null)i=tc(),ge.updateQueue=i,i.events=[e];else{var s=i.events;s===null?i.events=[e]:s.push(e)}}function gg(e){var i=pn().memoizedState;return iS({ref:i,nextImpl:e}),function(){if((ze&2)!==0)throw Error(a(440));return i.impl.apply(void 0,arguments)}}function _g(e,i){return ac(4,2,e,i)}function vg(e,i){return ac(4,4,e,i)}function xg(e,i){if(typeof i=="function"){e=e();var s=i(e);return function(){typeof s=="function"?s():i(null)}}if(i!=null)return e=e(),i.current=e,function(){i.current=null}}function yg(e,i,s){s=s!=null?s.concat([e]):null,ac(4,4,xg.bind(null,i,e),s)}function If(){}function Sg(e,i){var s=pn();i=i===void 0?null:i;var o=s.memoizedState;return i!==null&&Tf(i,o[1])?o[0]:(s.memoizedState=[e,i],e)}function Mg(e,i){var s=pn();i=i===void 0?null:i;var o=s.memoizedState;if(i!==null&&Tf(i,o[1]))return o[0];if(o=e(),Ps){Bt(!0);try{e()}finally{Bt(!1)}}return s.memoizedState=[o,i],o}function zf(e,i,s){return s===void 0||(ua&1073741824)!==0&&(we&261930)===0?e.memoizedState=i:(e.memoizedState=s,e=b0(),ge.lanes|=e,Ja|=e,s)}function bg(e,i,s,o){return ai(s,i)?s:vr.current!==null?(e=zf(e,s,o),ai(e,i)||(vn=!0),e):(ua&42)===0||(ua&1073741824)!==0&&(we&261930)===0?(vn=!0,e.memoizedState=s):(e=b0(),ge.lanes|=e,Ja|=e,i)}function Eg(e,i,s,o,u){var p=F.p;F.p=p!==0&&8>p?p:8;var M=P.T,N={};P.T=N,Hf(e,!1,i,s);try{var H=u(),nt=P.S;if(nt!==null&&nt(N,H),H!==null&&typeof H=="object"&&typeof H.then=="function"){var mt=Jy(H,o);Io(e,i,mt,ui(e))}else Io(e,i,o,ui(e))}catch(St){Io(e,i,{then:function(){},status:"rejected",reason:St},ui())}finally{F.p=p,M!==null&&N.types!==null&&(M.types=N.types),P.T=M}}function aS(){}function Bf(e,i,s,o){if(e.tag!==5)throw Error(a(476));var u=Tg(e).queue;Eg(e,u,i,ct,s===null?aS:function(){return Ag(e),s(o)})}function Tg(e){var i=e.memoizedState;if(i!==null)return i;i={memoizedState:ct,baseState:ct,baseQueue:null,queue:{pending:null,lanes:0,dispatch:null,lastRenderedReducer:fa,lastRenderedState:ct},next:null};var s={};return i.next={memoizedState:s,baseState:s,baseQueue:null,queue:{pending:null,lanes:0,dispatch:null,lastRenderedReducer:fa,lastRenderedState:s},next:null},e.memoizedState=i,e=e.alternate,e!==null&&(e.memoizedState=i),i}function Ag(e){var i=Tg(e);i.next===null&&(i=e.alternate.memoizedState),Io(e,i.next.queue,{},ui())}function Ff(){return Dn($o)}function wg(){return pn().memoizedState}function Rg(){return pn().memoizedState}function sS(e){for(var i=e.return;i!==null;){switch(i.tag){case 24:case 3:var s=ui();e=Wa(s);var o=qa(i,e,s);o!==null&&($n(o,i,s),Do(o,i,s)),i={cache:pf()},e.payload=i;return}i=i.return}}function rS(e,i,s){var o=ui();s={lane:o,revertLane:0,gesture:null,action:s,hasEagerState:!1,eagerState:null,next:null},sc(e)?Ng(i,s):(s=nf(e,i,s,o),s!==null&&($n(s,e,o),Dg(s,i,o)))}function Cg(e,i,s){var o=ui();Io(e,i,s,o)}function Io(e,i,s,o){var u={lane:o,revertLane:0,gesture:null,action:s,hasEagerState:!1,eagerState:null,next:null};if(sc(e))Ng(i,u);else{var p=e.alternate;if(e.lanes===0&&(p===null||p.lanes===0)&&(p=i.lastRenderedReducer,p!==null))try{var M=i.lastRenderedState,N=p(M,s);if(u.hasEagerState=!0,u.eagerState=N,ai(N,M))return Fl(e,i,u,0),Je===null&&Bl(),!1}catch{}finally{}if(s=nf(e,i,u,o),s!==null)return $n(s,e,o),Dg(s,i,o),!0}return!1}function Hf(e,i,s,o){if(o={lane:2,revertLane:vd(),gesture:null,action:o,hasEagerState:!1,eagerState:null,next:null},sc(e)){if(i)throw Error(a(479))}else i=nf(e,s,o,2),i!==null&&$n(i,e,2)}function sc(e){var i=e.alternate;return e===ge||i!==null&&i===ge}function Ng(e,i){xr=Jl=!0;var s=e.pending;s===null?i.next=i:(i.next=s.next,s.next=i),e.pending=i}function Dg(e,i,s){if((s&4194048)!==0){var o=i.lanes;o&=e.pendingLanes,s|=o,i.lanes=s,mi(e,s)}}var zo={readContext:Dn,use:ec,useCallback:cn,useContext:cn,useEffect:cn,useImperativeHandle:cn,useLayoutEffect:cn,useInsertionEffect:cn,useMemo:cn,useReducer:cn,useRef:cn,useState:cn,useDebugValue:cn,useDeferredValue:cn,useTransition:cn,useSyncExternalStore:cn,useId:cn,useHostTransitionStatus:cn,useFormState:cn,useActionState:cn,useOptimistic:cn,useMemoCache:cn,useCacheRefresh:cn};zo.useEffectEvent=cn;var Ug={readContext:Dn,use:ec,useCallback:function(e,i){return jn().memoizedState=[e,i===void 0?null:i],e},useContext:Dn,useEffect:mg,useImperativeHandle:function(e,i,s){s=s!=null?s.concat([e]):null,ic(4194308,4,xg.bind(null,i,e),s)},useLayoutEffect:function(e,i){return ic(4194308,4,e,i)},useInsertionEffect:function(e,i){ic(4,2,e,i)},useMemo:function(e,i){var s=jn();i=i===void 0?null:i;var o=e();if(Ps){Bt(!0);try{e()}finally{Bt(!1)}}return s.memoizedState=[o,i],o},useReducer:function(e,i,s){var o=jn();if(s!==void 0){var u=s(i);if(Ps){Bt(!0);try{s(i)}finally{Bt(!1)}}}else u=i;return o.memoizedState=o.baseState=u,e={pending:null,lanes:0,dispatch:null,lastRenderedReducer:e,lastRenderedState:u},o.queue=e,e=e.dispatch=rS.bind(null,ge,e),[o.memoizedState,e]},useRef:function(e){var i=jn();return e={current:e},i.memoizedState=e},useState:function(e){e=Lf(e);var i=e.queue,s=Cg.bind(null,ge,i);return i.dispatch=s,[e.memoizedState,s]},useDebugValue:If,useDeferredValue:function(e,i){var s=jn();return zf(s,e,i)},useTransition:function(){var e=Lf(!1);return e=Eg.bind(null,ge,e.queue,!0,!1),jn().memoizedState=e,[!1,e]},useSyncExternalStore:function(e,i,s){var o=ge,u=jn();if(Ce){if(s===void 0)throw Error(a(407));s=s()}else{if(s=i(),Je===null)throw Error(a(349));(we&127)!==0||$m(o,i,s)}u.memoizedState=s;var p={value:s,getSnapshot:i};return u.queue=p,mg(eg.bind(null,o,p,e),[e]),o.flags|=2048,Sr(9,{destroy:void 0},tg.bind(null,o,p,s,i),null),s},useId:function(){var e=jn(),i=Je.identifierPrefix;if(Ce){var s=ki,o=Vi;s=(o&~(1<<32-Ft(o)-1)).toString(32)+s,i="_"+i+"R_"+s,s=$l++,0<s&&(i+="H"+s.toString(32)),i+="_"}else s=$y++,i="_"+i+"r_"+s.toString(32)+"_";return e.memoizedState=i},useHostTransitionStatus:Ff,useFormState:ug,useActionState:ug,useOptimistic:function(e){var i=jn();i.memoizedState=i.baseState=e;var s={pending:null,lanes:0,dispatch:null,lastRenderedReducer:null,lastRenderedState:null};return i.queue=s,i=Hf.bind(null,ge,!0,s),s.dispatch=i,[e,i]},useMemoCache:Nf,useCacheRefresh:function(){return jn().memoizedState=sS.bind(null,ge)},useEffectEvent:function(e){var i=jn(),s={impl:e};return i.memoizedState=s,function(){if((ze&2)!==0)throw Error(a(440));return s.impl.apply(void 0,arguments)}}},Gf={readContext:Dn,use:ec,useCallback:Sg,useContext:Dn,useEffect:Pf,useImperativeHandle:yg,useInsertionEffect:_g,useLayoutEffect:vg,useMemo:Mg,useReducer:nc,useRef:pg,useState:function(){return nc(fa)},useDebugValue:If,useDeferredValue:function(e,i){var s=pn();return bg(s,Ye.memoizedState,e,i)},useTransition:function(){var e=nc(fa)[0],i=pn().memoizedState;return[typeof e=="boolean"?e:Po(e),i]},useSyncExternalStore:Jm,useId:wg,useHostTransitionStatus:Ff,useFormState:fg,useActionState:fg,useOptimistic:function(e,i){var s=pn();return ag(s,Ye,e,i)},useMemoCache:Nf,useCacheRefresh:Rg};Gf.useEffectEvent=gg;var Lg={readContext:Dn,use:ec,useCallback:Sg,useContext:Dn,useEffect:Pf,useImperativeHandle:yg,useInsertionEffect:_g,useLayoutEffect:vg,useMemo:Mg,useReducer:Uf,useRef:pg,useState:function(){return Uf(fa)},useDebugValue:If,useDeferredValue:function(e,i){var s=pn();return Ye===null?zf(s,e,i):bg(s,Ye.memoizedState,e,i)},useTransition:function(){var e=Uf(fa)[0],i=pn().memoizedState;return[typeof e=="boolean"?e:Po(e),i]},useSyncExternalStore:Jm,useId:wg,useHostTransitionStatus:Ff,useFormState:hg,useActionState:hg,useOptimistic:function(e,i){var s=pn();return Ye!==null?ag(s,Ye,e,i):(s.baseState=e,[e,s.queue.dispatch])},useMemoCache:Nf,useCacheRefresh:Rg};Lg.useEffectEvent=gg;function Vf(e,i,s,o){i=e.memoizedState,s=s(o,i),s=s==null?i:_({},i,s),e.memoizedState=s,e.lanes===0&&(e.updateQueue.baseState=s)}var kf={enqueueSetState:function(e,i,s){e=e._reactInternals;var o=ui(),u=Wa(o);u.payload=i,s!=null&&(u.callback=s),i=qa(e,u,o),i!==null&&($n(i,e,o),Do(i,e,o))},enqueueReplaceState:function(e,i,s){e=e._reactInternals;var o=ui(),u=Wa(o);u.tag=1,u.payload=i,s!=null&&(u.callback=s),i=qa(e,u,o),i!==null&&($n(i,e,o),Do(i,e,o))},enqueueForceUpdate:function(e,i){e=e._reactInternals;var s=ui(),o=Wa(s);o.tag=2,i!=null&&(o.callback=i),i=qa(e,o,s),i!==null&&($n(i,e,s),Do(i,e,s))}};function Og(e,i,s,o,u,p,M){return e=e.stateNode,typeof e.shouldComponentUpdate=="function"?e.shouldComponentUpdate(o,p,M):i.prototype&&i.prototype.isPureReactComponent?!bo(s,o)||!bo(u,p):!0}function Pg(e,i,s,o){e=i.state,typeof i.componentWillReceiveProps=="function"&&i.componentWillReceiveProps(s,o),typeof i.UNSAFE_componentWillReceiveProps=="function"&&i.UNSAFE_componentWillReceiveProps(s,o),i.state!==e&&kf.enqueueReplaceState(i,i.state,null)}function Is(e,i){var s=i;if("ref"in i){s={};for(var o in i)o!=="ref"&&(s[o]=i[o])}if(e=e.defaultProps){s===i&&(s=_({},s));for(var u in e)s[u]===void 0&&(s[u]=e[u])}return s}function Ig(e){zl(e)}function zg(e){console.error(e)}function Bg(e){zl(e)}function rc(e,i){try{var s=e.onUncaughtError;s(i.value,{componentStack:i.stack})}catch(o){setTimeout(function(){throw o})}}function Fg(e,i,s){try{var o=e.onCaughtError;o(s.value,{componentStack:s.stack,errorBoundary:i.tag===1?i.stateNode:null})}catch(u){setTimeout(function(){throw u})}}function jf(e,i,s){return s=Wa(s),s.tag=3,s.payload={element:null},s.callback=function(){rc(e,i)},s}function Hg(e){return e=Wa(e),e.tag=3,e}function Gg(e,i,s,o){var u=s.type.getDerivedStateFromError;if(typeof u=="function"){var p=o.value;e.payload=function(){return u(p)},e.callback=function(){Fg(i,s,o)}}var M=s.stateNode;M!==null&&typeof M.componentDidCatch=="function"&&(e.callback=function(){Fg(i,s,o),typeof u!="function"&&($a===null?$a=new Set([this]):$a.add(this));var N=o.stack;this.componentDidCatch(o.value,{componentStack:N!==null?N:""})})}function oS(e,i,s,o,u){if(s.flags|=32768,o!==null&&typeof o=="object"&&typeof o.then=="function"){if(i=s.alternate,i!==null&&hr(i,s,u,!0),s=ri.current,s!==null){switch(s.tag){case 31:case 13:return Si===null?vc():s.alternate===null&&un===0&&(un=3),s.flags&=-257,s.flags|=65536,s.lanes=u,o===ql?s.flags|=16384:(i=s.updateQueue,i===null?s.updateQueue=new Set([o]):i.add(o),md(e,o,u)),!1;case 22:return s.flags|=65536,o===ql?s.flags|=16384:(i=s.updateQueue,i===null?(i={transitions:null,markerInstances:null,retryQueue:new Set([o])},s.updateQueue=i):(s=i.retryQueue,s===null?i.retryQueue=new Set([o]):s.add(o)),md(e,o,u)),!1}throw Error(a(435,s.tag))}return md(e,o,u),vc(),!1}if(Ce)return i=ri.current,i!==null?((i.flags&65536)===0&&(i.flags|=256),i.flags|=65536,i.lanes=u,o!==cf&&(e=Error(a(422),{cause:o}),Ao(_i(e,s)))):(o!==cf&&(i=Error(a(423),{cause:o}),Ao(_i(i,s))),e=e.current.alternate,e.flags|=65536,u&=-u,e.lanes|=u,o=_i(o,s),u=jf(e.stateNode,o,u),yf(e,u),un!==4&&(un=2)),!1;var p=Error(a(520),{cause:o});if(p=_i(p,s),Xo===null?Xo=[p]:Xo.push(p),un!==4&&(un=2),i===null)return!0;o=_i(o,s),s=i;do{switch(s.tag){case 3:return s.flags|=65536,e=u&-u,s.lanes|=e,e=jf(s.stateNode,o,e),yf(s,e),!1;case 1:if(i=s.type,p=s.stateNode,(s.flags&128)===0&&(typeof i.getDerivedStateFromError=="function"||p!==null&&typeof p.componentDidCatch=="function"&&($a===null||!$a.has(p))))return s.flags|=65536,u&=-u,s.lanes|=u,u=Hg(u),Gg(u,e,s,o),yf(s,u),!1}s=s.return}while(s!==null);return!1}var Xf=Error(a(461)),vn=!1;function Un(e,i,s,o){i.child=e===null?Xm(i,null,s,o):Os(i,e.child,s,o)}function Vg(e,i,s,o,u){s=s.render;var p=i.ref;if("ref"in o){var M={};for(var N in o)N!=="ref"&&(M[N]=o[N])}else M=o;return Ns(i),o=Af(e,i,s,M,p,u),N=wf(),e!==null&&!vn?(Rf(e,i,u),da(e,i,u)):(Ce&&N&&of(i),i.flags|=1,Un(e,i,o,u),i.child)}function kg(e,i,s,o,u){if(e===null){var p=s.type;return typeof p=="function"&&!af(p)&&p.defaultProps===void 0&&s.compare===null?(i.tag=15,i.type=p,jg(e,i,p,o,u)):(e=Gl(s.type,null,o,i,i.mode,u),e.ref=i.ref,e.return=i,i.child=e)}if(p=e.child,!$f(e,u)){var M=p.memoizedProps;if(s=s.compare,s=s!==null?s:bo,s(M,o)&&e.ref===i.ref)return da(e,i,u)}return i.flags|=1,e=ra(p,o),e.ref=i.ref,e.return=i,i.child=e}function jg(e,i,s,o,u){if(e!==null){var p=e.memoizedProps;if(bo(p,o)&&e.ref===i.ref)if(vn=!1,i.pendingProps=o=p,$f(e,u))(e.flags&131072)!==0&&(vn=!0);else return i.lanes=e.lanes,da(e,i,u)}return Wf(e,i,s,o,u)}function Xg(e,i,s,o){var u=o.children,p=e!==null?e.memoizedState:null;if(e===null&&i.stateNode===null&&(i.stateNode={_visibility:1,_pendingMarkers:null,_retryCache:null,_transitions:null}),o.mode==="hidden"){if((i.flags&128)!==0){if(p=p!==null?p.baseLanes|s:s,e!==null){for(o=i.child=e.child,u=0;o!==null;)u=u|o.lanes|o.childLanes,o=o.sibling;o=u&~p}else o=0,i.child=null;return Wg(e,i,p,s,o)}if((s&536870912)!==0)i.memoizedState={baseLanes:0,cachePool:null},e!==null&&Xl(i,p!==null?p.cachePool:null),p!==null?Ym(i,p):Mf(),Zm(i);else return o=i.lanes=536870912,Wg(e,i,p!==null?p.baseLanes|s:s,s,o)}else p!==null?(Xl(i,p.cachePool),Ym(i,p),Za(),i.memoizedState=null):(e!==null&&Xl(i,null),Mf(),Za());return Un(e,i,u,s),i.child}function Bo(e,i){return e!==null&&e.tag===22||i.stateNode!==null||(i.stateNode={_visibility:1,_pendingMarkers:null,_retryCache:null,_transitions:null}),i.sibling}function Wg(e,i,s,o,u){var p=gf();return p=p===null?null:{parent:gn._currentValue,pool:p},i.memoizedState={baseLanes:s,cachePool:p},e!==null&&Xl(i,null),Mf(),Zm(i),e!==null&&hr(e,i,o,!0),i.childLanes=u,null}function oc(e,i){return i=cc({mode:i.mode,children:i.children},e.mode),i.ref=e.ref,e.child=i,i.return=e,i}function qg(e,i,s){return Os(i,e.child,null,s),e=oc(i,i.pendingProps),e.flags|=2,oi(i),i.memoizedState=null,e}function lS(e,i,s){var o=i.pendingProps,u=(i.flags&128)!==0;if(i.flags&=-129,e===null){if(Ce){if(o.mode==="hidden")return e=oc(i,o),i.lanes=536870912,Bo(null,e);if(Ef(i),(e=en)?(e=s_(e,yi),e=e!==null&&e.data==="&"?e:null,e!==null&&(i.memoizedState={dehydrated:e,treeContext:Ga!==null?{id:Vi,overflow:ki}:null,retryLane:536870912,hydrationErrors:null},s=Nm(e),s.return=i,i.child=s,Nn=i,en=null)):e=null,e===null)throw ka(i);return i.lanes=536870912,null}return oc(i,o)}var p=e.memoizedState;if(p!==null){var M=p.dehydrated;if(Ef(i),u)if(i.flags&256)i.flags&=-257,i=qg(e,i,s);else if(i.memoizedState!==null)i.child=e.child,i.flags|=128,i=null;else throw Error(a(558));else if(vn||hr(e,i,s,!1),u=(s&e.childLanes)!==0,vn||u){if(o=Je,o!==null&&(M=ei(o,s),M!==0&&M!==p.retryLane))throw p.retryLane=M,As(e,M),$n(o,e,M),Xf;vc(),i=qg(e,i,s)}else e=p.treeContext,en=Mi(M.nextSibling),Nn=i,Ce=!0,Va=null,yi=!1,e!==null&&Lm(i,e),i=oc(i,o),i.flags|=4096;return i}return e=ra(e.child,{mode:o.mode,children:o.children}),e.ref=i.ref,i.child=e,e.return=i,e}function lc(e,i){var s=i.ref;if(s===null)e!==null&&e.ref!==null&&(i.flags|=4194816);else{if(typeof s!="function"&&typeof s!="object")throw Error(a(284));(e===null||e.ref!==s)&&(i.flags|=4194816)}}function Wf(e,i,s,o,u){return Ns(i),s=Af(e,i,s,o,void 0,u),o=wf(),e!==null&&!vn?(Rf(e,i,u),da(e,i,u)):(Ce&&o&&of(i),i.flags|=1,Un(e,i,s,u),i.child)}function Yg(e,i,s,o,u,p){return Ns(i),i.updateQueue=null,s=Qm(i,o,s,u),Km(e),o=wf(),e!==null&&!vn?(Rf(e,i,p),da(e,i,p)):(Ce&&o&&of(i),i.flags|=1,Un(e,i,s,p),i.child)}function Zg(e,i,s,o,u){if(Ns(i),i.stateNode===null){var p=cr,M=s.contextType;typeof M=="object"&&M!==null&&(p=Dn(M)),p=new s(o,p),i.memoizedState=p.state!==null&&p.state!==void 0?p.state:null,p.updater=kf,i.stateNode=p,p._reactInternals=i,p=i.stateNode,p.props=o,p.state=i.memoizedState,p.refs={},vf(i),M=s.contextType,p.context=typeof M=="object"&&M!==null?Dn(M):cr,p.state=i.memoizedState,M=s.getDerivedStateFromProps,typeof M=="function"&&(Vf(i,s,M,o),p.state=i.memoizedState),typeof s.getDerivedStateFromProps=="function"||typeof p.getSnapshotBeforeUpdate=="function"||typeof p.UNSAFE_componentWillMount!="function"&&typeof p.componentWillMount!="function"||(M=p.state,typeof p.componentWillMount=="function"&&p.componentWillMount(),typeof p.UNSAFE_componentWillMount=="function"&&p.UNSAFE_componentWillMount(),M!==p.state&&kf.enqueueReplaceState(p,p.state,null),Lo(i,o,p,u),Uo(),p.state=i.memoizedState),typeof p.componentDidMount=="function"&&(i.flags|=4194308),o=!0}else if(e===null){p=i.stateNode;var N=i.memoizedProps,H=Is(s,N);p.props=H;var nt=p.context,mt=s.contextType;M=cr,typeof mt=="object"&&mt!==null&&(M=Dn(mt));var St=s.getDerivedStateFromProps;mt=typeof St=="function"||typeof p.getSnapshotBeforeUpdate=="function",N=i.pendingProps!==N,mt||typeof p.UNSAFE_componentWillReceiveProps!="function"&&typeof p.componentWillReceiveProps!="function"||(N||nt!==M)&&Pg(i,p,o,M),Xa=!1;var lt=i.memoizedState;p.state=lt,Lo(i,o,p,u),Uo(),nt=i.memoizedState,N||lt!==nt||Xa?(typeof St=="function"&&(Vf(i,s,St,o),nt=i.memoizedState),(H=Xa||Og(i,s,H,o,lt,nt,M))?(mt||typeof p.UNSAFE_componentWillMount!="function"&&typeof p.componentWillMount!="function"||(typeof p.componentWillMount=="function"&&p.componentWillMount(),typeof p.UNSAFE_componentWillMount=="function"&&p.UNSAFE_componentWillMount()),typeof p.componentDidMount=="function"&&(i.flags|=4194308)):(typeof p.componentDidMount=="function"&&(i.flags|=4194308),i.memoizedProps=o,i.memoizedState=nt),p.props=o,p.state=nt,p.context=M,o=H):(typeof p.componentDidMount=="function"&&(i.flags|=4194308),o=!1)}else{p=i.stateNode,xf(e,i),M=i.memoizedProps,mt=Is(s,M),p.props=mt,St=i.pendingProps,lt=p.context,nt=s.contextType,H=cr,typeof nt=="object"&&nt!==null&&(H=Dn(nt)),N=s.getDerivedStateFromProps,(nt=typeof N=="function"||typeof p.getSnapshotBeforeUpdate=="function")||typeof p.UNSAFE_componentWillReceiveProps!="function"&&typeof p.componentWillReceiveProps!="function"||(M!==St||lt!==H)&&Pg(i,p,o,H),Xa=!1,lt=i.memoizedState,p.state=lt,Lo(i,o,p,u),Uo();var ft=i.memoizedState;M!==St||lt!==ft||Xa||e!==null&&e.dependencies!==null&&kl(e.dependencies)?(typeof N=="function"&&(Vf(i,s,N,o),ft=i.memoizedState),(mt=Xa||Og(i,s,mt,o,lt,ft,H)||e!==null&&e.dependencies!==null&&kl(e.dependencies))?(nt||typeof p.UNSAFE_componentWillUpdate!="function"&&typeof p.componentWillUpdate!="function"||(typeof p.componentWillUpdate=="function"&&p.componentWillUpdate(o,ft,H),typeof p.UNSAFE_componentWillUpdate=="function"&&p.UNSAFE_componentWillUpdate(o,ft,H)),typeof p.componentDidUpdate=="function"&&(i.flags|=4),typeof p.getSnapshotBeforeUpdate=="function"&&(i.flags|=1024)):(typeof p.componentDidUpdate!="function"||M===e.memoizedProps&&lt===e.memoizedState||(i.flags|=4),typeof p.getSnapshotBeforeUpdate!="function"||M===e.memoizedProps&&lt===e.memoizedState||(i.flags|=1024),i.memoizedProps=o,i.memoizedState=ft),p.props=o,p.state=ft,p.context=H,o=mt):(typeof p.componentDidUpdate!="function"||M===e.memoizedProps&&lt===e.memoizedState||(i.flags|=4),typeof p.getSnapshotBeforeUpdate!="function"||M===e.memoizedProps&&lt===e.memoizedState||(i.flags|=1024),o=!1)}return p=o,lc(e,i),o=(i.flags&128)!==0,p||o?(p=i.stateNode,s=o&&typeof s.getDerivedStateFromError!="function"?null:p.render(),i.flags|=1,e!==null&&o?(i.child=Os(i,e.child,null,u),i.child=Os(i,null,s,u)):Un(e,i,s,u),i.memoizedState=p.state,e=i.child):e=da(e,i,u),e}function Kg(e,i,s,o){return Rs(),i.flags|=256,Un(e,i,s,o),i.child}var qf={dehydrated:null,treeContext:null,retryLane:0,hydrationErrors:null};function Yf(e){return{baseLanes:e,cachePool:Fm()}}function Zf(e,i,s){return e=e!==null?e.childLanes&~s:0,i&&(e|=ci),e}function Qg(e,i,s){var o=i.pendingProps,u=!1,p=(i.flags&128)!==0,M;if((M=p)||(M=e!==null&&e.memoizedState===null?!1:(hn.current&2)!==0),M&&(u=!0,i.flags&=-129),M=(i.flags&32)!==0,i.flags&=-33,e===null){if(Ce){if(u?Ya(i):Za(),(e=en)?(e=s_(e,yi),e=e!==null&&e.data!=="&"?e:null,e!==null&&(i.memoizedState={dehydrated:e,treeContext:Ga!==null?{id:Vi,overflow:ki}:null,retryLane:536870912,hydrationErrors:null},s=Nm(e),s.return=i,i.child=s,Nn=i,en=null)):e=null,e===null)throw ka(i);return Dd(e)?i.lanes=32:i.lanes=536870912,null}var N=o.children;return o=o.fallback,u?(Za(),u=i.mode,N=cc({mode:"hidden",children:N},u),o=ws(o,u,s,null),N.return=i,o.return=i,N.sibling=o,i.child=N,o=i.child,o.memoizedState=Yf(s),o.childLanes=Zf(e,M,s),i.memoizedState=qf,Bo(null,o)):(Ya(i),Kf(i,N))}var H=e.memoizedState;if(H!==null&&(N=H.dehydrated,N!==null)){if(p)i.flags&256?(Ya(i),i.flags&=-257,i=Qf(e,i,s)):i.memoizedState!==null?(Za(),i.child=e.child,i.flags|=128,i=null):(Za(),N=o.fallback,u=i.mode,o=cc({mode:"visible",children:o.children},u),N=ws(N,u,s,null),N.flags|=2,o.return=i,N.return=i,o.sibling=N,i.child=o,Os(i,e.child,null,s),o=i.child,o.memoizedState=Yf(s),o.childLanes=Zf(e,M,s),i.memoizedState=qf,i=Bo(null,o));else if(Ya(i),Dd(N)){if(M=N.nextSibling&&N.nextSibling.dataset,M)var nt=M.dgst;M=nt,o=Error(a(419)),o.stack="",o.digest=M,Ao({value:o,source:null,stack:null}),i=Qf(e,i,s)}else if(vn||hr(e,i,s,!1),M=(s&e.childLanes)!==0,vn||M){if(M=Je,M!==null&&(o=ei(M,s),o!==0&&o!==H.retryLane))throw H.retryLane=o,As(e,o),$n(M,e,o),Xf;Nd(N)||vc(),i=Qf(e,i,s)}else Nd(N)?(i.flags|=192,i.child=e.child,i=null):(e=H.treeContext,en=Mi(N.nextSibling),Nn=i,Ce=!0,Va=null,yi=!1,e!==null&&Lm(i,e),i=Kf(i,o.children),i.flags|=4096);return i}return u?(Za(),N=o.fallback,u=i.mode,H=e.child,nt=H.sibling,o=ra(H,{mode:"hidden",children:o.children}),o.subtreeFlags=H.subtreeFlags&65011712,nt!==null?N=ra(nt,N):(N=ws(N,u,s,null),N.flags|=2),N.return=i,o.return=i,o.sibling=N,i.child=o,Bo(null,o),o=i.child,N=e.child.memoizedState,N===null?N=Yf(s):(u=N.cachePool,u!==null?(H=gn._currentValue,u=u.parent!==H?{parent:H,pool:H}:u):u=Fm(),N={baseLanes:N.baseLanes|s,cachePool:u}),o.memoizedState=N,o.childLanes=Zf(e,M,s),i.memoizedState=qf,Bo(e.child,o)):(Ya(i),s=e.child,e=s.sibling,s=ra(s,{mode:"visible",children:o.children}),s.return=i,s.sibling=null,e!==null&&(M=i.deletions,M===null?(i.deletions=[e],i.flags|=16):M.push(e)),i.child=s,i.memoizedState=null,s)}function Kf(e,i){return i=cc({mode:"visible",children:i},e.mode),i.return=e,e.child=i}function cc(e,i){return e=si(22,e,null,i),e.lanes=0,e}function Qf(e,i,s){return Os(i,e.child,null,s),e=Kf(i,i.pendingProps.children),e.flags|=2,i.memoizedState=null,e}function Jg(e,i,s){e.lanes|=i;var o=e.alternate;o!==null&&(o.lanes|=i),df(e.return,i,s)}function Jf(e,i,s,o,u,p){var M=e.memoizedState;M===null?e.memoizedState={isBackwards:i,rendering:null,renderingStartTime:0,last:o,tail:s,tailMode:u,treeForkCount:p}:(M.isBackwards=i,M.rendering=null,M.renderingStartTime=0,M.last=o,M.tail=s,M.tailMode=u,M.treeForkCount=p)}function $g(e,i,s){var o=i.pendingProps,u=o.revealOrder,p=o.tail;o=o.children;var M=hn.current,N=(M&2)!==0;if(N?(M=M&1|2,i.flags|=128):M&=1,Mt(hn,M),Un(e,i,o,s),o=Ce?To:0,!N&&e!==null&&(e.flags&128)!==0)t:for(e=i.child;e!==null;){if(e.tag===13)e.memoizedState!==null&&Jg(e,s,i);else if(e.tag===19)Jg(e,s,i);else if(e.child!==null){e.child.return=e,e=e.child;continue}if(e===i)break t;for(;e.sibling===null;){if(e.return===null||e.return===i)break t;e=e.return}e.sibling.return=e.return,e=e.sibling}switch(u){case"forwards":for(s=i.child,u=null;s!==null;)e=s.alternate,e!==null&&Ql(e)===null&&(u=s),s=s.sibling;s=u,s===null?(u=i.child,i.child=null):(u=s.sibling,s.sibling=null),Jf(i,!1,u,s,p,o);break;case"backwards":case"unstable_legacy-backwards":for(s=null,u=i.child,i.child=null;u!==null;){if(e=u.alternate,e!==null&&Ql(e)===null){i.child=u;break}e=u.sibling,u.sibling=s,s=u,u=e}Jf(i,!0,s,null,p,o);break;case"together":Jf(i,!1,null,null,void 0,o);break;default:i.memoizedState=null}return i.child}function da(e,i,s){if(e!==null&&(i.dependencies=e.dependencies),Ja|=i.lanes,(s&i.childLanes)===0)if(e!==null){if(hr(e,i,s,!1),(s&i.childLanes)===0)return null}else return null;if(e!==null&&i.child!==e.child)throw Error(a(153));if(i.child!==null){for(e=i.child,s=ra(e,e.pendingProps),i.child=s,s.return=i;e.sibling!==null;)e=e.sibling,s=s.sibling=ra(e,e.pendingProps),s.return=i;s.sibling=null}return i.child}function $f(e,i){return(e.lanes&i)!==0?!0:(e=e.dependencies,!!(e!==null&&kl(e)))}function cS(e,i,s){switch(i.tag){case 3:Tt(i,i.stateNode.containerInfo),ja(i,gn,e.memoizedState.cache),Rs();break;case 27:case 5:re(i);break;case 4:Tt(i,i.stateNode.containerInfo);break;case 10:ja(i,i.type,i.memoizedProps.value);break;case 31:if(i.memoizedState!==null)return i.flags|=128,Ef(i),null;break;case 13:var o=i.memoizedState;if(o!==null)return o.dehydrated!==null?(Ya(i),i.flags|=128,null):(s&i.child.childLanes)!==0?Qg(e,i,s):(Ya(i),e=da(e,i,s),e!==null?e.sibling:null);Ya(i);break;case 19:var u=(e.flags&128)!==0;if(o=(s&i.childLanes)!==0,o||(hr(e,i,s,!1),o=(s&i.childLanes)!==0),u){if(o)return $g(e,i,s);i.flags|=128}if(u=i.memoizedState,u!==null&&(u.rendering=null,u.tail=null,u.lastEffect=null),Mt(hn,hn.current),o)break;return null;case 22:return i.lanes=0,Xg(e,i,s,i.pendingProps);case 24:ja(i,gn,e.memoizedState.cache)}return da(e,i,s)}function t0(e,i,s){if(e!==null)if(e.memoizedProps!==i.pendingProps)vn=!0;else{if(!$f(e,s)&&(i.flags&128)===0)return vn=!1,cS(e,i,s);vn=(e.flags&131072)!==0}else vn=!1,Ce&&(i.flags&1048576)!==0&&Um(i,To,i.index);switch(i.lanes=0,i.tag){case 16:t:{var o=i.pendingProps;if(e=Us(i.elementType),i.type=e,typeof e=="function")af(e)?(o=Is(e,o),i.tag=1,i=Zg(null,i,e,o,s)):(i.tag=0,i=Wf(null,i,e,o,s));else{if(e!=null){var u=e.$$typeof;if(u===U){i.tag=11,i=Vg(null,i,e,o,s);break t}else if(u===B){i.tag=14,i=kg(null,i,e,o,s);break t}}throw i=gt(e)||e,Error(a(306,i,""))}}return i;case 0:return Wf(e,i,i.type,i.pendingProps,s);case 1:return o=i.type,u=Is(o,i.pendingProps),Zg(e,i,o,u,s);case 3:t:{if(Tt(i,i.stateNode.containerInfo),e===null)throw Error(a(387));o=i.pendingProps;var p=i.memoizedState;u=p.element,xf(e,i),Lo(i,o,null,s);var M=i.memoizedState;if(o=M.cache,ja(i,gn,o),o!==p.cache&&hf(i,[gn],s,!0),Uo(),o=M.element,p.isDehydrated)if(p={element:o,isDehydrated:!1,cache:M.cache},i.updateQueue.baseState=p,i.memoizedState=p,i.flags&256){i=Kg(e,i,o,s);break t}else if(o!==u){u=_i(Error(a(424)),i),Ao(u),i=Kg(e,i,o,s);break t}else{switch(e=i.stateNode.containerInfo,e.nodeType){case 9:e=e.body;break;default:e=e.nodeName==="HTML"?e.ownerDocument.body:e}for(en=Mi(e.firstChild),Nn=i,Ce=!0,Va=null,yi=!0,s=Xm(i,null,o,s),i.child=s;s;)s.flags=s.flags&-3|4096,s=s.sibling}else{if(Rs(),o===u){i=da(e,i,s);break t}Un(e,i,o,s)}i=i.child}return i;case 26:return lc(e,i),e===null?(s=f_(i.type,null,i.pendingProps,null))?i.memoizedState=s:Ce||(s=i.type,e=i.pendingProps,o=Tc(st.current).createElement(s),o[dn]=i,o[Cn]=e,Ln(o,s,e),mn(o),i.stateNode=o):i.memoizedState=f_(i.type,e.memoizedProps,i.pendingProps,e.memoizedState),null;case 27:return re(i),e===null&&Ce&&(o=i.stateNode=l_(i.type,i.pendingProps,st.current),Nn=i,yi=!0,u=en,is(i.type)?(Ud=u,en=Mi(o.firstChild)):en=u),Un(e,i,i.pendingProps.children,s),lc(e,i),e===null&&(i.flags|=4194304),i.child;case 5:return e===null&&Ce&&((u=o=en)&&(o=FS(o,i.type,i.pendingProps,yi),o!==null?(i.stateNode=o,Nn=i,en=Mi(o.firstChild),yi=!1,u=!0):u=!1),u||ka(i)),re(i),u=i.type,p=i.pendingProps,M=e!==null?e.memoizedProps:null,o=p.children,wd(u,p)?o=null:M!==null&&wd(u,M)&&(i.flags|=32),i.memoizedState!==null&&(u=Af(e,i,tS,null,null,s),$o._currentValue=u),lc(e,i),Un(e,i,o,s),i.child;case 6:return e===null&&Ce&&((e=s=en)&&(s=HS(s,i.pendingProps,yi),s!==null?(i.stateNode=s,Nn=i,en=null,e=!0):e=!1),e||ka(i)),null;case 13:return Qg(e,i,s);case 4:return Tt(i,i.stateNode.containerInfo),o=i.pendingProps,e===null?i.child=Os(i,null,o,s):Un(e,i,o,s),i.child;case 11:return Vg(e,i,i.type,i.pendingProps,s);case 7:return Un(e,i,i.pendingProps,s),i.child;case 8:return Un(e,i,i.pendingProps.children,s),i.child;case 12:return Un(e,i,i.pendingProps.children,s),i.child;case 10:return o=i.pendingProps,ja(i,i.type,o.value),Un(e,i,o.children,s),i.child;case 9:return u=i.type._context,o=i.pendingProps.children,Ns(i),u=Dn(u),o=o(u),i.flags|=1,Un(e,i,o,s),i.child;case 14:return kg(e,i,i.type,i.pendingProps,s);case 15:return jg(e,i,i.type,i.pendingProps,s);case 19:return $g(e,i,s);case 31:return lS(e,i,s);case 22:return Xg(e,i,s,i.pendingProps);case 24:return Ns(i),o=Dn(gn),e===null?(u=gf(),u===null&&(u=Je,p=pf(),u.pooledCache=p,p.refCount++,p!==null&&(u.pooledCacheLanes|=s),u=p),i.memoizedState={parent:o,cache:u},vf(i),ja(i,gn,u)):((e.lanes&s)!==0&&(xf(e,i),Lo(i,null,null,s),Uo()),u=e.memoizedState,p=i.memoizedState,u.parent!==o?(u={parent:o,cache:o},i.memoizedState=u,i.lanes===0&&(i.memoizedState=i.updateQueue.baseState=u),ja(i,gn,o)):(o=p.cache,ja(i,gn,o),o!==u.cache&&hf(i,[gn],s,!0))),Un(e,i,i.pendingProps.children,s),i.child;case 29:throw i.pendingProps}throw Error(a(156,i.tag))}function ha(e){e.flags|=4}function td(e,i,s,o,u){if((i=(e.mode&32)!==0)&&(i=!1),i){if(e.flags|=16777216,(u&335544128)===u)if(e.stateNode.complete)e.flags|=8192;else if(w0())e.flags|=8192;else throw Ls=ql,_f}else e.flags&=-16777217}function e0(e,i){if(i.type!=="stylesheet"||(i.state.loading&4)!==0)e.flags&=-16777217;else if(e.flags|=16777216,!g_(i))if(w0())e.flags|=8192;else throw Ls=ql,_f}function uc(e,i){i!==null&&(e.flags|=4),e.flags&16384&&(i=e.tag!==22?Et():536870912,e.lanes|=i,Tr|=i)}function Fo(e,i){if(!Ce)switch(e.tailMode){case"hidden":i=e.tail;for(var s=null;i!==null;)i.alternate!==null&&(s=i),i=i.sibling;s===null?e.tail=null:s.sibling=null;break;case"collapsed":s=e.tail;for(var o=null;s!==null;)s.alternate!==null&&(o=s),s=s.sibling;o===null?i||e.tail===null?e.tail=null:e.tail.sibling=null:o.sibling=null}}function nn(e){var i=e.alternate!==null&&e.alternate.child===e.child,s=0,o=0;if(i)for(var u=e.child;u!==null;)s|=u.lanes|u.childLanes,o|=u.subtreeFlags&65011712,o|=u.flags&65011712,u.return=e,u=u.sibling;else for(u=e.child;u!==null;)s|=u.lanes|u.childLanes,o|=u.subtreeFlags,o|=u.flags,u.return=e,u=u.sibling;return e.subtreeFlags|=o,e.childLanes=s,i}function uS(e,i,s){var o=i.pendingProps;switch(lf(i),i.tag){case 16:case 15:case 0:case 11:case 7:case 8:case 12:case 9:case 14:return nn(i),null;case 1:return nn(i),null;case 3:return s=i.stateNode,o=null,e!==null&&(o=e.memoizedState.cache),i.memoizedState.cache!==o&&(i.flags|=2048),ca(gn),Wt(),s.pendingContext&&(s.context=s.pendingContext,s.pendingContext=null),(e===null||e.child===null)&&(dr(i)?ha(i):e===null||e.memoizedState.isDehydrated&&(i.flags&256)===0||(i.flags|=1024,uf())),nn(i),null;case 26:var u=i.type,p=i.memoizedState;return e===null?(ha(i),p!==null?(nn(i),e0(i,p)):(nn(i),td(i,u,null,o,s))):p?p!==e.memoizedState?(ha(i),nn(i),e0(i,p)):(nn(i),i.flags&=-16777217):(e=e.memoizedProps,e!==o&&ha(i),nn(i),td(i,u,e,o,s)),null;case 27:if(ie(i),s=st.current,u=i.type,e!==null&&i.stateNode!=null)e.memoizedProps!==o&&ha(i);else{if(!o){if(i.stateNode===null)throw Error(a(166));return nn(i),null}e=Rt.current,dr(i)?Om(i):(e=l_(u,o,s),i.stateNode=e,ha(i))}return nn(i),null;case 5:if(ie(i),u=i.type,e!==null&&i.stateNode!=null)e.memoizedProps!==o&&ha(i);else{if(!o){if(i.stateNode===null)throw Error(a(166));return nn(i),null}if(p=Rt.current,dr(i))Om(i);else{var M=Tc(st.current);switch(p){case 1:p=M.createElementNS("http://www.w3.org/2000/svg",u);break;case 2:p=M.createElementNS("http://www.w3.org/1998/Math/MathML",u);break;default:switch(u){case"svg":p=M.createElementNS("http://www.w3.org/2000/svg",u);break;case"math":p=M.createElementNS("http://www.w3.org/1998/Math/MathML",u);break;case"script":p=M.createElement("div"),p.innerHTML="<script><\/script>",p=p.removeChild(p.firstChild);break;case"select":p=typeof o.is=="string"?M.createElement("select",{is:o.is}):M.createElement("select"),o.multiple?p.multiple=!0:o.size&&(p.size=o.size);break;default:p=typeof o.is=="string"?M.createElement(u,{is:o.is}):M.createElement(u)}}p[dn]=i,p[Cn]=o;t:for(M=i.child;M!==null;){if(M.tag===5||M.tag===6)p.appendChild(M.stateNode);else if(M.tag!==4&&M.tag!==27&&M.child!==null){M.child.return=M,M=M.child;continue}if(M===i)break t;for(;M.sibling===null;){if(M.return===null||M.return===i)break t;M=M.return}M.sibling.return=M.return,M=M.sibling}i.stateNode=p;t:switch(Ln(p,u,o),u){case"button":case"input":case"select":case"textarea":o=!!o.autoFocus;break t;case"img":o=!0;break t;default:o=!1}o&&ha(i)}}return nn(i),td(i,i.type,e===null?null:e.memoizedProps,i.pendingProps,s),null;case 6:if(e&&i.stateNode!=null)e.memoizedProps!==o&&ha(i);else{if(typeof o!="string"&&i.stateNode===null)throw Error(a(166));if(e=st.current,dr(i)){if(e=i.stateNode,s=i.memoizedProps,o=null,u=Nn,u!==null)switch(u.tag){case 27:case 5:o=u.memoizedProps}e[dn]=i,e=!!(e.nodeValue===s||o!==null&&o.suppressHydrationWarning===!0||Q0(e.nodeValue,s)),e||ka(i,!0)}else e=Tc(e).createTextNode(o),e[dn]=i,i.stateNode=e}return nn(i),null;case 31:if(s=i.memoizedState,e===null||e.memoizedState!==null){if(o=dr(i),s!==null){if(e===null){if(!o)throw Error(a(318));if(e=i.memoizedState,e=e!==null?e.dehydrated:null,!e)throw Error(a(557));e[dn]=i}else Rs(),(i.flags&128)===0&&(i.memoizedState=null),i.flags|=4;nn(i),e=!1}else s=uf(),e!==null&&e.memoizedState!==null&&(e.memoizedState.hydrationErrors=s),e=!0;if(!e)return i.flags&256?(oi(i),i):(oi(i),null);if((i.flags&128)!==0)throw Error(a(558))}return nn(i),null;case 13:if(o=i.memoizedState,e===null||e.memoizedState!==null&&e.memoizedState.dehydrated!==null){if(u=dr(i),o!==null&&o.dehydrated!==null){if(e===null){if(!u)throw Error(a(318));if(u=i.memoizedState,u=u!==null?u.dehydrated:null,!u)throw Error(a(317));u[dn]=i}else Rs(),(i.flags&128)===0&&(i.memoizedState=null),i.flags|=4;nn(i),u=!1}else u=uf(),e!==null&&e.memoizedState!==null&&(e.memoizedState.hydrationErrors=u),u=!0;if(!u)return i.flags&256?(oi(i),i):(oi(i),null)}return oi(i),(i.flags&128)!==0?(i.lanes=s,i):(s=o!==null,e=e!==null&&e.memoizedState!==null,s&&(o=i.child,u=null,o.alternate!==null&&o.alternate.memoizedState!==null&&o.alternate.memoizedState.cachePool!==null&&(u=o.alternate.memoizedState.cachePool.pool),p=null,o.memoizedState!==null&&o.memoizedState.cachePool!==null&&(p=o.memoizedState.cachePool.pool),p!==u&&(o.flags|=2048)),s!==e&&s&&(i.child.flags|=8192),uc(i,i.updateQueue),nn(i),null);case 4:return Wt(),e===null&&Md(i.stateNode.containerInfo),nn(i),null;case 10:return ca(i.type),nn(i),null;case 19:if(Q(hn),o=i.memoizedState,o===null)return nn(i),null;if(u=(i.flags&128)!==0,p=o.rendering,p===null)if(u)Fo(o,!1);else{if(un!==0||e!==null&&(e.flags&128)!==0)for(e=i.child;e!==null;){if(p=Ql(e),p!==null){for(i.flags|=128,Fo(o,!1),e=p.updateQueue,i.updateQueue=e,uc(i,e),i.subtreeFlags=0,e=s,s=i.child;s!==null;)Cm(s,e),s=s.sibling;return Mt(hn,hn.current&1|2),Ce&&oa(i,o.treeForkCount),i.child}e=e.sibling}o.tail!==null&&Dt()>mc&&(i.flags|=128,u=!0,Fo(o,!1),i.lanes=4194304)}else{if(!u)if(e=Ql(p),e!==null){if(i.flags|=128,u=!0,e=e.updateQueue,i.updateQueue=e,uc(i,e),Fo(o,!0),o.tail===null&&o.tailMode==="hidden"&&!p.alternate&&!Ce)return nn(i),null}else 2*Dt()-o.renderingStartTime>mc&&s!==536870912&&(i.flags|=128,u=!0,Fo(o,!1),i.lanes=4194304);o.isBackwards?(p.sibling=i.child,i.child=p):(e=o.last,e!==null?e.sibling=p:i.child=p,o.last=p)}return o.tail!==null?(e=o.tail,o.rendering=e,o.tail=e.sibling,o.renderingStartTime=Dt(),e.sibling=null,s=hn.current,Mt(hn,u?s&1|2:s&1),Ce&&oa(i,o.treeForkCount),e):(nn(i),null);case 22:case 23:return oi(i),bf(),o=i.memoizedState!==null,e!==null?e.memoizedState!==null!==o&&(i.flags|=8192):o&&(i.flags|=8192),o?(s&536870912)!==0&&(i.flags&128)===0&&(nn(i),i.subtreeFlags&6&&(i.flags|=8192)):nn(i),s=i.updateQueue,s!==null&&uc(i,s.retryQueue),s=null,e!==null&&e.memoizedState!==null&&e.memoizedState.cachePool!==null&&(s=e.memoizedState.cachePool.pool),o=null,i.memoizedState!==null&&i.memoizedState.cachePool!==null&&(o=i.memoizedState.cachePool.pool),o!==s&&(i.flags|=2048),e!==null&&Q(Ds),null;case 24:return s=null,e!==null&&(s=e.memoizedState.cache),i.memoizedState.cache!==s&&(i.flags|=2048),ca(gn),nn(i),null;case 25:return null;case 30:return null}throw Error(a(156,i.tag))}function fS(e,i){switch(lf(i),i.tag){case 1:return e=i.flags,e&65536?(i.flags=e&-65537|128,i):null;case 3:return ca(gn),Wt(),e=i.flags,(e&65536)!==0&&(e&128)===0?(i.flags=e&-65537|128,i):null;case 26:case 27:case 5:return ie(i),null;case 31:if(i.memoizedState!==null){if(oi(i),i.alternate===null)throw Error(a(340));Rs()}return e=i.flags,e&65536?(i.flags=e&-65537|128,i):null;case 13:if(oi(i),e=i.memoizedState,e!==null&&e.dehydrated!==null){if(i.alternate===null)throw Error(a(340));Rs()}return e=i.flags,e&65536?(i.flags=e&-65537|128,i):null;case 19:return Q(hn),null;case 4:return Wt(),null;case 10:return ca(i.type),null;case 22:case 23:return oi(i),bf(),e!==null&&Q(Ds),e=i.flags,e&65536?(i.flags=e&-65537|128,i):null;case 24:return ca(gn),null;case 25:return null;default:return null}}function n0(e,i){switch(lf(i),i.tag){case 3:ca(gn),Wt();break;case 26:case 27:case 5:ie(i);break;case 4:Wt();break;case 31:i.memoizedState!==null&&oi(i);break;case 13:oi(i);break;case 19:Q(hn);break;case 10:ca(i.type);break;case 22:case 23:oi(i),bf(),e!==null&&Q(Ds);break;case 24:ca(gn)}}function Ho(e,i){try{var s=i.updateQueue,o=s!==null?s.lastEffect:null;if(o!==null){var u=o.next;s=u;do{if((s.tag&e)===e){o=void 0;var p=s.create,M=s.inst;o=p(),M.destroy=o}s=s.next}while(s!==u)}}catch(N){ke(i,i.return,N)}}function Ka(e,i,s){try{var o=i.updateQueue,u=o!==null?o.lastEffect:null;if(u!==null){var p=u.next;o=p;do{if((o.tag&e)===e){var M=o.inst,N=M.destroy;if(N!==void 0){M.destroy=void 0,u=i;var H=s,nt=N;try{nt()}catch(mt){ke(u,H,mt)}}}o=o.next}while(o!==p)}}catch(mt){ke(i,i.return,mt)}}function i0(e){var i=e.updateQueue;if(i!==null){var s=e.stateNode;try{qm(i,s)}catch(o){ke(e,e.return,o)}}}function a0(e,i,s){s.props=Is(e.type,e.memoizedProps),s.state=e.memoizedState;try{s.componentWillUnmount()}catch(o){ke(e,i,o)}}function Go(e,i){try{var s=e.ref;if(s!==null){switch(e.tag){case 26:case 27:case 5:var o=e.stateNode;break;case 30:o=e.stateNode;break;default:o=e.stateNode}typeof s=="function"?e.refCleanup=s(o):s.current=o}}catch(u){ke(e,i,u)}}function ji(e,i){var s=e.ref,o=e.refCleanup;if(s!==null)if(typeof o=="function")try{o()}catch(u){ke(e,i,u)}finally{e.refCleanup=null,e=e.alternate,e!=null&&(e.refCleanup=null)}else if(typeof s=="function")try{s(null)}catch(u){ke(e,i,u)}else s.current=null}function s0(e){var i=e.type,s=e.memoizedProps,o=e.stateNode;try{t:switch(i){case"button":case"input":case"select":case"textarea":s.autoFocus&&o.focus();break t;case"img":s.src?o.src=s.src:s.srcSet&&(o.srcset=s.srcSet)}}catch(u){ke(e,e.return,u)}}function ed(e,i,s){try{var o=e.stateNode;LS(o,e.type,s,i),o[Cn]=i}catch(u){ke(e,e.return,u)}}function r0(e){return e.tag===5||e.tag===3||e.tag===26||e.tag===27&&is(e.type)||e.tag===4}function nd(e){t:for(;;){for(;e.sibling===null;){if(e.return===null||r0(e.return))return null;e=e.return}for(e.sibling.return=e.return,e=e.sibling;e.tag!==5&&e.tag!==6&&e.tag!==18;){if(e.tag===27&&is(e.type)||e.flags&2||e.child===null||e.tag===4)continue t;e.child.return=e,e=e.child}if(!(e.flags&2))return e.stateNode}}function id(e,i,s){var o=e.tag;if(o===5||o===6)e=e.stateNode,i?(s.nodeType===9?s.body:s.nodeName==="HTML"?s.ownerDocument.body:s).insertBefore(e,i):(i=s.nodeType===9?s.body:s.nodeName==="HTML"?s.ownerDocument.body:s,i.appendChild(e),s=s._reactRootContainer,s!=null||i.onclick!==null||(i.onclick=aa));else if(o!==4&&(o===27&&is(e.type)&&(s=e.stateNode,i=null),e=e.child,e!==null))for(id(e,i,s),e=e.sibling;e!==null;)id(e,i,s),e=e.sibling}function fc(e,i,s){var o=e.tag;if(o===5||o===6)e=e.stateNode,i?s.insertBefore(e,i):s.appendChild(e);else if(o!==4&&(o===27&&is(e.type)&&(s=e.stateNode),e=e.child,e!==null))for(fc(e,i,s),e=e.sibling;e!==null;)fc(e,i,s),e=e.sibling}function o0(e){var i=e.stateNode,s=e.memoizedProps;try{for(var o=e.type,u=i.attributes;u.length;)i.removeAttributeNode(u[0]);Ln(i,o,s),i[dn]=e,i[Cn]=s}catch(p){ke(e,e.return,p)}}var pa=!1,xn=!1,ad=!1,l0=typeof WeakSet=="function"?WeakSet:Set,An=null;function dS(e,i){if(e=e.containerInfo,Td=Uc,e=ym(e),Ku(e)){if("selectionStart"in e)var s={start:e.selectionStart,end:e.selectionEnd};else t:{s=(s=e.ownerDocument)&&s.defaultView||window;var o=s.getSelection&&s.getSelection();if(o&&o.rangeCount!==0){s=o.anchorNode;var u=o.anchorOffset,p=o.focusNode;o=o.focusOffset;try{s.nodeType,p.nodeType}catch{s=null;break t}var M=0,N=-1,H=-1,nt=0,mt=0,St=e,lt=null;e:for(;;){for(var ft;St!==s||u!==0&&St.nodeType!==3||(N=M+u),St!==p||o!==0&&St.nodeType!==3||(H=M+o),St.nodeType===3&&(M+=St.nodeValue.length),(ft=St.firstChild)!==null;)lt=St,St=ft;for(;;){if(St===e)break e;if(lt===s&&++nt===u&&(N=M),lt===p&&++mt===o&&(H=M),(ft=St.nextSibling)!==null)break;St=lt,lt=St.parentNode}St=ft}s=N===-1||H===-1?null:{start:N,end:H}}else s=null}s=s||{start:0,end:0}}else s=null;for(Ad={focusedElem:e,selectionRange:s},Uc=!1,An=i;An!==null;)if(i=An,e=i.child,(i.subtreeFlags&1028)!==0&&e!==null)e.return=i,An=e;else for(;An!==null;){switch(i=An,p=i.alternate,e=i.flags,i.tag){case 0:if((e&4)!==0&&(e=i.updateQueue,e=e!==null?e.events:null,e!==null))for(s=0;s<e.length;s++)u=e[s],u.ref.impl=u.nextImpl;break;case 11:case 15:break;case 1:if((e&1024)!==0&&p!==null){e=void 0,s=i,u=p.memoizedProps,p=p.memoizedState,o=s.stateNode;try{var Qt=Is(s.type,u);e=o.getSnapshotBeforeUpdate(Qt,p),o.__reactInternalSnapshotBeforeUpdate=e}catch(oe){ke(s,s.return,oe)}}break;case 3:if((e&1024)!==0){if(e=i.stateNode.containerInfo,s=e.nodeType,s===9)Cd(e);else if(s===1)switch(e.nodeName){case"HEAD":case"HTML":case"BODY":Cd(e);break;default:e.textContent=""}}break;case 5:case 26:case 27:case 6:case 4:case 17:break;default:if((e&1024)!==0)throw Error(a(163))}if(e=i.sibling,e!==null){e.return=i.return,An=e;break}An=i.return}}function c0(e,i,s){var o=s.flags;switch(s.tag){case 0:case 11:case 15:ga(e,s),o&4&&Ho(5,s);break;case 1:if(ga(e,s),o&4)if(e=s.stateNode,i===null)try{e.componentDidMount()}catch(M){ke(s,s.return,M)}else{var u=Is(s.type,i.memoizedProps);i=i.memoizedState;try{e.componentDidUpdate(u,i,e.__reactInternalSnapshotBeforeUpdate)}catch(M){ke(s,s.return,M)}}o&64&&i0(s),o&512&&Go(s,s.return);break;case 3:if(ga(e,s),o&64&&(e=s.updateQueue,e!==null)){if(i=null,s.child!==null)switch(s.child.tag){case 27:case 5:i=s.child.stateNode;break;case 1:i=s.child.stateNode}try{qm(e,i)}catch(M){ke(s,s.return,M)}}break;case 27:i===null&&o&4&&o0(s);case 26:case 5:ga(e,s),i===null&&o&4&&s0(s),o&512&&Go(s,s.return);break;case 12:ga(e,s);break;case 31:ga(e,s),o&4&&d0(e,s);break;case 13:ga(e,s),o&4&&h0(e,s),o&64&&(e=s.memoizedState,e!==null&&(e=e.dehydrated,e!==null&&(s=SS.bind(null,s),GS(e,s))));break;case 22:if(o=s.memoizedState!==null||pa,!o){i=i!==null&&i.memoizedState!==null||xn,u=pa;var p=xn;pa=o,(xn=i)&&!p?_a(e,s,(s.subtreeFlags&8772)!==0):ga(e,s),pa=u,xn=p}break;case 30:break;default:ga(e,s)}}function u0(e){var i=e.alternate;i!==null&&(e.alternate=null,u0(i)),e.child=null,e.deletions=null,e.sibling=null,e.tag===5&&(i=e.stateNode,i!==null&&mo(i)),e.stateNode=null,e.return=null,e.dependencies=null,e.memoizedProps=null,e.memoizedState=null,e.pendingProps=null,e.stateNode=null,e.updateQueue=null}var on=null,Zn=!1;function ma(e,i,s){for(s=s.child;s!==null;)f0(e,i,s),s=s.sibling}function f0(e,i,s){if(pt&&typeof pt.onCommitFiberUnmount=="function")try{pt.onCommitFiberUnmount(dt,s)}catch{}switch(s.tag){case 26:xn||ji(s,i),ma(e,i,s),s.memoizedState?s.memoizedState.count--:s.stateNode&&(s=s.stateNode,s.parentNode.removeChild(s));break;case 27:xn||ji(s,i);var o=on,u=Zn;is(s.type)&&(on=s.stateNode,Zn=!1),ma(e,i,s),Ko(s.stateNode),on=o,Zn=u;break;case 5:xn||ji(s,i);case 6:if(o=on,u=Zn,on=null,ma(e,i,s),on=o,Zn=u,on!==null)if(Zn)try{(on.nodeType===9?on.body:on.nodeName==="HTML"?on.ownerDocument.body:on).removeChild(s.stateNode)}catch(p){ke(s,i,p)}else try{on.removeChild(s.stateNode)}catch(p){ke(s,i,p)}break;case 18:on!==null&&(Zn?(e=on,i_(e.nodeType===9?e.body:e.nodeName==="HTML"?e.ownerDocument.body:e,s.stateNode),Lr(e)):i_(on,s.stateNode));break;case 4:o=on,u=Zn,on=s.stateNode.containerInfo,Zn=!0,ma(e,i,s),on=o,Zn=u;break;case 0:case 11:case 14:case 15:Ka(2,s,i),xn||Ka(4,s,i),ma(e,i,s);break;case 1:xn||(ji(s,i),o=s.stateNode,typeof o.componentWillUnmount=="function"&&a0(s,i,o)),ma(e,i,s);break;case 21:ma(e,i,s);break;case 22:xn=(o=xn)||s.memoizedState!==null,ma(e,i,s),xn=o;break;default:ma(e,i,s)}}function d0(e,i){if(i.memoizedState===null&&(e=i.alternate,e!==null&&(e=e.memoizedState,e!==null))){e=e.dehydrated;try{Lr(e)}catch(s){ke(i,i.return,s)}}}function h0(e,i){if(i.memoizedState===null&&(e=i.alternate,e!==null&&(e=e.memoizedState,e!==null&&(e=e.dehydrated,e!==null))))try{Lr(e)}catch(s){ke(i,i.return,s)}}function hS(e){switch(e.tag){case 31:case 13:case 19:var i=e.stateNode;return i===null&&(i=e.stateNode=new l0),i;case 22:return e=e.stateNode,i=e._retryCache,i===null&&(i=e._retryCache=new l0),i;default:throw Error(a(435,e.tag))}}function dc(e,i){var s=hS(e);i.forEach(function(o){if(!s.has(o)){s.add(o);var u=MS.bind(null,e,o);o.then(u,u)}})}function Kn(e,i){var s=i.deletions;if(s!==null)for(var o=0;o<s.length;o++){var u=s[o],p=e,M=i,N=M;t:for(;N!==null;){switch(N.tag){case 27:if(is(N.type)){on=N.stateNode,Zn=!1;break t}break;case 5:on=N.stateNode,Zn=!1;break t;case 3:case 4:on=N.stateNode.containerInfo,Zn=!0;break t}N=N.return}if(on===null)throw Error(a(160));f0(p,M,u),on=null,Zn=!1,p=u.alternate,p!==null&&(p.return=null),u.return=null}if(i.subtreeFlags&13886)for(i=i.child;i!==null;)p0(i,e),i=i.sibling}var Oi=null;function p0(e,i){var s=e.alternate,o=e.flags;switch(e.tag){case 0:case 11:case 14:case 15:Kn(i,e),Qn(e),o&4&&(Ka(3,e,e.return),Ho(3,e),Ka(5,e,e.return));break;case 1:Kn(i,e),Qn(e),o&512&&(xn||s===null||ji(s,s.return)),o&64&&pa&&(e=e.updateQueue,e!==null&&(o=e.callbacks,o!==null&&(s=e.shared.hiddenCallbacks,e.shared.hiddenCallbacks=s===null?o:s.concat(o))));break;case 26:var u=Oi;if(Kn(i,e),Qn(e),o&512&&(xn||s===null||ji(s,s.return)),o&4){var p=s!==null?s.memoizedState:null;if(o=e.memoizedState,s===null)if(o===null)if(e.stateNode===null){t:{o=e.type,s=e.memoizedProps,u=u.ownerDocument||u;e:switch(o){case"title":p=u.getElementsByTagName("title")[0],(!p||p[Pa]||p[dn]||p.namespaceURI==="http://www.w3.org/2000/svg"||p.hasAttribute("itemprop"))&&(p=u.createElement(o),u.head.insertBefore(p,u.querySelector("head > title"))),Ln(p,o,s),p[dn]=e,mn(p),o=p;break t;case"link":var M=p_("link","href",u).get(o+(s.href||""));if(M){for(var N=0;N<M.length;N++)if(p=M[N],p.getAttribute("href")===(s.href==null||s.href===""?null:s.href)&&p.getAttribute("rel")===(s.rel==null?null:s.rel)&&p.getAttribute("title")===(s.title==null?null:s.title)&&p.getAttribute("crossorigin")===(s.crossOrigin==null?null:s.crossOrigin)){M.splice(N,1);break e}}p=u.createElement(o),Ln(p,o,s),u.head.appendChild(p);break;case"meta":if(M=p_("meta","content",u).get(o+(s.content||""))){for(N=0;N<M.length;N++)if(p=M[N],p.getAttribute("content")===(s.content==null?null:""+s.content)&&p.getAttribute("name")===(s.name==null?null:s.name)&&p.getAttribute("property")===(s.property==null?null:s.property)&&p.getAttribute("http-equiv")===(s.httpEquiv==null?null:s.httpEquiv)&&p.getAttribute("charset")===(s.charSet==null?null:s.charSet)){M.splice(N,1);break e}}p=u.createElement(o),Ln(p,o,s),u.head.appendChild(p);break;default:throw Error(a(468,o))}p[dn]=e,mn(p),o=p}e.stateNode=o}else m_(u,e.type,e.stateNode);else e.stateNode=h_(u,o,e.memoizedProps);else p!==o?(p===null?s.stateNode!==null&&(s=s.stateNode,s.parentNode.removeChild(s)):p.count--,o===null?m_(u,e.type,e.stateNode):h_(u,o,e.memoizedProps)):o===null&&e.stateNode!==null&&ed(e,e.memoizedProps,s.memoizedProps)}break;case 27:Kn(i,e),Qn(e),o&512&&(xn||s===null||ji(s,s.return)),s!==null&&o&4&&ed(e,e.memoizedProps,s.memoizedProps);break;case 5:if(Kn(i,e),Qn(e),o&512&&(xn||s===null||ji(s,s.return)),e.flags&32){u=e.stateNode;try{ii(u,"")}catch(Qt){ke(e,e.return,Qt)}}o&4&&e.stateNode!=null&&(u=e.memoizedProps,ed(e,u,s!==null?s.memoizedProps:u)),o&1024&&(ad=!0);break;case 6:if(Kn(i,e),Qn(e),o&4){if(e.stateNode===null)throw Error(a(162));o=e.memoizedProps,s=e.stateNode;try{s.nodeValue=o}catch(Qt){ke(e,e.return,Qt)}}break;case 3:if(Rc=null,u=Oi,Oi=Ac(i.containerInfo),Kn(i,e),Oi=u,Qn(e),o&4&&s!==null&&s.memoizedState.isDehydrated)try{Lr(i.containerInfo)}catch(Qt){ke(e,e.return,Qt)}ad&&(ad=!1,m0(e));break;case 4:o=Oi,Oi=Ac(e.stateNode.containerInfo),Kn(i,e),Qn(e),Oi=o;break;case 12:Kn(i,e),Qn(e);break;case 31:Kn(i,e),Qn(e),o&4&&(o=e.updateQueue,o!==null&&(e.updateQueue=null,dc(e,o)));break;case 13:Kn(i,e),Qn(e),e.child.flags&8192&&e.memoizedState!==null!=(s!==null&&s.memoizedState!==null)&&(pc=Dt()),o&4&&(o=e.updateQueue,o!==null&&(e.updateQueue=null,dc(e,o)));break;case 22:u=e.memoizedState!==null;var H=s!==null&&s.memoizedState!==null,nt=pa,mt=xn;if(pa=nt||u,xn=mt||H,Kn(i,e),xn=mt,pa=nt,Qn(e),o&8192)t:for(i=e.stateNode,i._visibility=u?i._visibility&-2:i._visibility|1,u&&(s===null||H||pa||xn||zs(e)),s=null,i=e;;){if(i.tag===5||i.tag===26){if(s===null){H=s=i;try{if(p=H.stateNode,u)M=p.style,typeof M.setProperty=="function"?M.setProperty("display","none","important"):M.display="none";else{N=H.stateNode;var St=H.memoizedProps.style,lt=St!=null&&St.hasOwnProperty("display")?St.display:null;N.style.display=lt==null||typeof lt=="boolean"?"":(""+lt).trim()}}catch(Qt){ke(H,H.return,Qt)}}}else if(i.tag===6){if(s===null){H=i;try{H.stateNode.nodeValue=u?"":H.memoizedProps}catch(Qt){ke(H,H.return,Qt)}}}else if(i.tag===18){if(s===null){H=i;try{var ft=H.stateNode;u?a_(ft,!0):a_(H.stateNode,!1)}catch(Qt){ke(H,H.return,Qt)}}}else if((i.tag!==22&&i.tag!==23||i.memoizedState===null||i===e)&&i.child!==null){i.child.return=i,i=i.child;continue}if(i===e)break t;for(;i.sibling===null;){if(i.return===null||i.return===e)break t;s===i&&(s=null),i=i.return}s===i&&(s=null),i.sibling.return=i.return,i=i.sibling}o&4&&(o=e.updateQueue,o!==null&&(s=o.retryQueue,s!==null&&(o.retryQueue=null,dc(e,s))));break;case 19:Kn(i,e),Qn(e),o&4&&(o=e.updateQueue,o!==null&&(e.updateQueue=null,dc(e,o)));break;case 30:break;case 21:break;default:Kn(i,e),Qn(e)}}function Qn(e){var i=e.flags;if(i&2){try{for(var s,o=e.return;o!==null;){if(r0(o)){s=o;break}o=o.return}if(s==null)throw Error(a(160));switch(s.tag){case 27:var u=s.stateNode,p=nd(e);fc(e,p,u);break;case 5:var M=s.stateNode;s.flags&32&&(ii(M,""),s.flags&=-33);var N=nd(e);fc(e,N,M);break;case 3:case 4:var H=s.stateNode.containerInfo,nt=nd(e);id(e,nt,H);break;default:throw Error(a(161))}}catch(mt){ke(e,e.return,mt)}e.flags&=-3}i&4096&&(e.flags&=-4097)}function m0(e){if(e.subtreeFlags&1024)for(e=e.child;e!==null;){var i=e;m0(i),i.tag===5&&i.flags&1024&&i.stateNode.reset(),e=e.sibling}}function ga(e,i){if(i.subtreeFlags&8772)for(i=i.child;i!==null;)c0(e,i.alternate,i),i=i.sibling}function zs(e){for(e=e.child;e!==null;){var i=e;switch(i.tag){case 0:case 11:case 14:case 15:Ka(4,i,i.return),zs(i);break;case 1:ji(i,i.return);var s=i.stateNode;typeof s.componentWillUnmount=="function"&&a0(i,i.return,s),zs(i);break;case 27:Ko(i.stateNode);case 26:case 5:ji(i,i.return),zs(i);break;case 22:i.memoizedState===null&&zs(i);break;case 30:zs(i);break;default:zs(i)}e=e.sibling}}function _a(e,i,s){for(s=s&&(i.subtreeFlags&8772)!==0,i=i.child;i!==null;){var o=i.alternate,u=e,p=i,M=p.flags;switch(p.tag){case 0:case 11:case 15:_a(u,p,s),Ho(4,p);break;case 1:if(_a(u,p,s),o=p,u=o.stateNode,typeof u.componentDidMount=="function")try{u.componentDidMount()}catch(nt){ke(o,o.return,nt)}if(o=p,u=o.updateQueue,u!==null){var N=o.stateNode;try{var H=u.shared.hiddenCallbacks;if(H!==null)for(u.shared.hiddenCallbacks=null,u=0;u<H.length;u++)Wm(H[u],N)}catch(nt){ke(o,o.return,nt)}}s&&M&64&&i0(p),Go(p,p.return);break;case 27:o0(p);case 26:case 5:_a(u,p,s),s&&o===null&&M&4&&s0(p),Go(p,p.return);break;case 12:_a(u,p,s);break;case 31:_a(u,p,s),s&&M&4&&d0(u,p);break;case 13:_a(u,p,s),s&&M&4&&h0(u,p);break;case 22:p.memoizedState===null&&_a(u,p,s),Go(p,p.return);break;case 30:break;default:_a(u,p,s)}i=i.sibling}}function sd(e,i){var s=null;e!==null&&e.memoizedState!==null&&e.memoizedState.cachePool!==null&&(s=e.memoizedState.cachePool.pool),e=null,i.memoizedState!==null&&i.memoizedState.cachePool!==null&&(e=i.memoizedState.cachePool.pool),e!==s&&(e!=null&&e.refCount++,s!=null&&wo(s))}function rd(e,i){e=null,i.alternate!==null&&(e=i.alternate.memoizedState.cache),i=i.memoizedState.cache,i!==e&&(i.refCount++,e!=null&&wo(e))}function Pi(e,i,s,o){if(i.subtreeFlags&10256)for(i=i.child;i!==null;)g0(e,i,s,o),i=i.sibling}function g0(e,i,s,o){var u=i.flags;switch(i.tag){case 0:case 11:case 15:Pi(e,i,s,o),u&2048&&Ho(9,i);break;case 1:Pi(e,i,s,o);break;case 3:Pi(e,i,s,o),u&2048&&(e=null,i.alternate!==null&&(e=i.alternate.memoizedState.cache),i=i.memoizedState.cache,i!==e&&(i.refCount++,e!=null&&wo(e)));break;case 12:if(u&2048){Pi(e,i,s,o),e=i.stateNode;try{var p=i.memoizedProps,M=p.id,N=p.onPostCommit;typeof N=="function"&&N(M,i.alternate===null?"mount":"update",e.passiveEffectDuration,-0)}catch(H){ke(i,i.return,H)}}else Pi(e,i,s,o);break;case 31:Pi(e,i,s,o);break;case 13:Pi(e,i,s,o);break;case 23:break;case 22:p=i.stateNode,M=i.alternate,i.memoizedState!==null?p._visibility&2?Pi(e,i,s,o):Vo(e,i):p._visibility&2?Pi(e,i,s,o):(p._visibility|=2,Mr(e,i,s,o,(i.subtreeFlags&10256)!==0||!1)),u&2048&&sd(M,i);break;case 24:Pi(e,i,s,o),u&2048&&rd(i.alternate,i);break;default:Pi(e,i,s,o)}}function Mr(e,i,s,o,u){for(u=u&&((i.subtreeFlags&10256)!==0||!1),i=i.child;i!==null;){var p=e,M=i,N=s,H=o,nt=M.flags;switch(M.tag){case 0:case 11:case 15:Mr(p,M,N,H,u),Ho(8,M);break;case 23:break;case 22:var mt=M.stateNode;M.memoizedState!==null?mt._visibility&2?Mr(p,M,N,H,u):Vo(p,M):(mt._visibility|=2,Mr(p,M,N,H,u)),u&&nt&2048&&sd(M.alternate,M);break;case 24:Mr(p,M,N,H,u),u&&nt&2048&&rd(M.alternate,M);break;default:Mr(p,M,N,H,u)}i=i.sibling}}function Vo(e,i){if(i.subtreeFlags&10256)for(i=i.child;i!==null;){var s=e,o=i,u=o.flags;switch(o.tag){case 22:Vo(s,o),u&2048&&sd(o.alternate,o);break;case 24:Vo(s,o),u&2048&&rd(o.alternate,o);break;default:Vo(s,o)}i=i.sibling}}var ko=8192;function br(e,i,s){if(e.subtreeFlags&ko)for(e=e.child;e!==null;)_0(e,i,s),e=e.sibling}function _0(e,i,s){switch(e.tag){case 26:br(e,i,s),e.flags&ko&&e.memoizedState!==null&&$S(s,Oi,e.memoizedState,e.memoizedProps);break;case 5:br(e,i,s);break;case 3:case 4:var o=Oi;Oi=Ac(e.stateNode.containerInfo),br(e,i,s),Oi=o;break;case 22:e.memoizedState===null&&(o=e.alternate,o!==null&&o.memoizedState!==null?(o=ko,ko=16777216,br(e,i,s),ko=o):br(e,i,s));break;default:br(e,i,s)}}function v0(e){var i=e.alternate;if(i!==null&&(e=i.child,e!==null)){i.child=null;do i=e.sibling,e.sibling=null,e=i;while(e!==null)}}function jo(e){var i=e.deletions;if((e.flags&16)!==0){if(i!==null)for(var s=0;s<i.length;s++){var o=i[s];An=o,y0(o,e)}v0(e)}if(e.subtreeFlags&10256)for(e=e.child;e!==null;)x0(e),e=e.sibling}function x0(e){switch(e.tag){case 0:case 11:case 15:jo(e),e.flags&2048&&Ka(9,e,e.return);break;case 3:jo(e);break;case 12:jo(e);break;case 22:var i=e.stateNode;e.memoizedState!==null&&i._visibility&2&&(e.return===null||e.return.tag!==13)?(i._visibility&=-3,hc(e)):jo(e);break;default:jo(e)}}function hc(e){var i=e.deletions;if((e.flags&16)!==0){if(i!==null)for(var s=0;s<i.length;s++){var o=i[s];An=o,y0(o,e)}v0(e)}for(e=e.child;e!==null;){switch(i=e,i.tag){case 0:case 11:case 15:Ka(8,i,i.return),hc(i);break;case 22:s=i.stateNode,s._visibility&2&&(s._visibility&=-3,hc(i));break;default:hc(i)}e=e.sibling}}function y0(e,i){for(;An!==null;){var s=An;switch(s.tag){case 0:case 11:case 15:Ka(8,s,i);break;case 23:case 22:if(s.memoizedState!==null&&s.memoizedState.cachePool!==null){var o=s.memoizedState.cachePool.pool;o!=null&&o.refCount++}break;case 24:wo(s.memoizedState.cache)}if(o=s.child,o!==null)o.return=s,An=o;else t:for(s=e;An!==null;){o=An;var u=o.sibling,p=o.return;if(u0(o),o===s){An=null;break t}if(u!==null){u.return=p,An=u;break t}An=p}}}var pS={getCacheForType:function(e){var i=Dn(gn),s=i.data.get(e);return s===void 0&&(s=e(),i.data.set(e,s)),s},cacheSignal:function(){return Dn(gn).controller.signal}},mS=typeof WeakMap=="function"?WeakMap:Map,ze=0,Je=null,Te=null,we=0,Ve=0,li=null,Qa=!1,Er=!1,od=!1,va=0,un=0,Ja=0,Bs=0,ld=0,ci=0,Tr=0,Xo=null,Jn=null,cd=!1,pc=0,S0=0,mc=1/0,gc=null,$a=null,bn=0,ts=null,Ar=null,xa=0,ud=0,fd=null,M0=null,Wo=0,dd=null;function ui(){return(ze&2)!==0&&we!==0?we&-we:P.T!==null?vd():fo()}function b0(){if(ci===0)if((we&536870912)===0||Ce){var e=be;be<<=1,(be&3932160)===0&&(be=262144),ci=e}else ci=536870912;return e=ri.current,e!==null&&(e.flags|=32),ci}function $n(e,i,s){(e===Je&&(Ve===2||Ve===9)||e.cancelPendingCommit!==null)&&(wr(e,0),es(e,we,ci,!1)),ue(e,s),((ze&2)===0||e!==Je)&&(e===Je&&((ze&2)===0&&(Bs|=s),un===4&&es(e,we,ci,!1)),Xi(e))}function E0(e,i,s){if((ze&6)!==0)throw Error(a(327));var o=!s&&(i&127)===0&&(i&e.expiredLanes)===0||jt(e,i),u=o?vS(e,i):pd(e,i,!0),p=o;do{if(u===0){Er&&!o&&es(e,i,0,!1);break}else{if(s=e.current.alternate,p&&!gS(s)){u=pd(e,i,!1),p=!1;continue}if(u===2){if(p=i,e.errorRecoveryDisabledLanes&p)var M=0;else M=e.pendingLanes&-536870913,M=M!==0?M:M&536870912?536870912:0;if(M!==0){i=M;t:{var N=e;u=Xo;var H=N.current.memoizedState.isDehydrated;if(H&&(wr(N,M).flags|=256),M=pd(N,M,!1),M!==2){if(od&&!H){N.errorRecoveryDisabledLanes|=p,Bs|=p,u=4;break t}p=Jn,Jn=u,p!==null&&(Jn===null?Jn=p:Jn.push.apply(Jn,p))}u=M}if(p=!1,u!==2)continue}}if(u===1){wr(e,0),es(e,i,0,!0);break}t:{switch(o=e,p=u,p){case 0:case 1:throw Error(a(345));case 4:if((i&4194048)!==i)break;case 6:es(o,i,ci,!Qa);break t;case 2:Jn=null;break;case 3:case 5:break;default:throw Error(a(329))}if((i&62914560)===i&&(u=pc+300-Dt(),10<u)){if(es(o,i,ci,!Qa),_t(o,0,!0)!==0)break t;xa=i,o.timeoutHandle=e_(T0.bind(null,o,s,Jn,gc,cd,i,ci,Bs,Tr,Qa,p,"Throttled",-0,0),u);break t}T0(o,s,Jn,gc,cd,i,ci,Bs,Tr,Qa,p,null,-0,0)}}break}while(!0);Xi(e)}function T0(e,i,s,o,u,p,M,N,H,nt,mt,St,lt,ft){if(e.timeoutHandle=-1,St=i.subtreeFlags,St&8192||(St&16785408)===16785408){St={stylesheets:null,count:0,imgCount:0,imgBytes:0,suspenseyImages:[],waitingForImages:!0,waitingForViewTransition:!1,unsuspend:aa},_0(i,p,St);var Qt=(p&62914560)===p?pc-Dt():(p&4194048)===p?S0-Dt():0;if(Qt=tM(St,Qt),Qt!==null){xa=p,e.cancelPendingCommit=Qt(L0.bind(null,e,i,p,s,o,u,M,N,H,mt,St,null,lt,ft)),es(e,p,M,!nt);return}}L0(e,i,p,s,o,u,M,N,H)}function gS(e){for(var i=e;;){var s=i.tag;if((s===0||s===11||s===15)&&i.flags&16384&&(s=i.updateQueue,s!==null&&(s=s.stores,s!==null)))for(var o=0;o<s.length;o++){var u=s[o],p=u.getSnapshot;u=u.value;try{if(!ai(p(),u))return!1}catch{return!1}}if(s=i.child,i.subtreeFlags&16384&&s!==null)s.return=i,i=s;else{if(i===e)break;for(;i.sibling===null;){if(i.return===null||i.return===e)return!0;i=i.return}i.sibling.return=i.return,i=i.sibling}}return!0}function es(e,i,s,o){i&=~ld,i&=~Bs,e.suspendedLanes|=i,e.pingedLanes&=~i,o&&(e.warmLanes|=i),o=e.expirationTimes;for(var u=i;0<u;){var p=31-Ft(u),M=1<<p;o[p]=-1,u&=~M}s!==0&&Ie(e,s,i)}function _c(){return(ze&6)===0?(qo(0),!1):!0}function hd(){if(Te!==null){if(Ve===0)var e=Te.return;else e=Te,la=Cs=null,Cf(e),_r=null,Co=0,e=Te;for(;e!==null;)n0(e.alternate,e),e=e.return;Te=null}}function wr(e,i){var s=e.timeoutHandle;s!==-1&&(e.timeoutHandle=-1,IS(s)),s=e.cancelPendingCommit,s!==null&&(e.cancelPendingCommit=null,s()),xa=0,hd(),Je=e,Te=s=ra(e.current,null),we=i,Ve=0,li=null,Qa=!1,Er=jt(e,i),od=!1,Tr=ci=ld=Bs=Ja=un=0,Jn=Xo=null,cd=!1,(i&8)!==0&&(i|=i&32);var o=e.entangledLanes;if(o!==0)for(e=e.entanglements,o&=i;0<o;){var u=31-Ft(o),p=1<<u;i|=e[u],o&=~p}return va=i,Bl(),s}function A0(e,i){ge=null,P.H=zo,i===gr||i===Wl?(i=Vm(),Ve=3):i===_f?(i=Vm(),Ve=4):Ve=i===Xf?8:i!==null&&typeof i=="object"&&typeof i.then=="function"?6:1,li=i,Te===null&&(un=1,rc(e,_i(i,e.current)))}function w0(){var e=ri.current;return e===null?!0:(we&4194048)===we?Si===null:(we&62914560)===we||(we&536870912)!==0?e===Si:!1}function R0(){var e=P.H;return P.H=zo,e===null?zo:e}function C0(){var e=P.A;return P.A=pS,e}function vc(){un=4,Qa||(we&4194048)!==we&&ri.current!==null||(Er=!0),(Ja&134217727)===0&&(Bs&134217727)===0||Je===null||es(Je,we,ci,!1)}function pd(e,i,s){var o=ze;ze|=2;var u=R0(),p=C0();(Je!==e||we!==i)&&(gc=null,wr(e,i)),i=!1;var M=un;t:do try{if(Ve!==0&&Te!==null){var N=Te,H=li;switch(Ve){case 8:hd(),M=6;break t;case 3:case 2:case 9:case 6:ri.current===null&&(i=!0);var nt=Ve;if(Ve=0,li=null,Rr(e,N,H,nt),s&&Er){M=0;break t}break;default:nt=Ve,Ve=0,li=null,Rr(e,N,H,nt)}}_S(),M=un;break}catch(mt){A0(e,mt)}while(!0);return i&&e.shellSuspendCounter++,la=Cs=null,ze=o,P.H=u,P.A=p,Te===null&&(Je=null,we=0,Bl()),M}function _S(){for(;Te!==null;)N0(Te)}function vS(e,i){var s=ze;ze|=2;var o=R0(),u=C0();Je!==e||we!==i?(gc=null,mc=Dt()+500,wr(e,i)):Er=jt(e,i);t:do try{if(Ve!==0&&Te!==null){i=Te;var p=li;e:switch(Ve){case 1:Ve=0,li=null,Rr(e,i,p,1);break;case 2:case 9:if(Hm(p)){Ve=0,li=null,D0(i);break}i=function(){Ve!==2&&Ve!==9||Je!==e||(Ve=7),Xi(e)},p.then(i,i);break t;case 3:Ve=7;break t;case 4:Ve=5;break t;case 7:Hm(p)?(Ve=0,li=null,D0(i)):(Ve=0,li=null,Rr(e,i,p,7));break;case 5:var M=null;switch(Te.tag){case 26:M=Te.memoizedState;case 5:case 27:var N=Te;if(M?g_(M):N.stateNode.complete){Ve=0,li=null;var H=N.sibling;if(H!==null)Te=H;else{var nt=N.return;nt!==null?(Te=nt,xc(nt)):Te=null}break e}}Ve=0,li=null,Rr(e,i,p,5);break;case 6:Ve=0,li=null,Rr(e,i,p,6);break;case 8:hd(),un=6;break t;default:throw Error(a(462))}}xS();break}catch(mt){A0(e,mt)}while(!0);return la=Cs=null,P.H=o,P.A=u,ze=s,Te!==null?0:(Je=null,we=0,Bl(),un)}function xS(){for(;Te!==null&&!ye();)N0(Te)}function N0(e){var i=t0(e.alternate,e,va);e.memoizedProps=e.pendingProps,i===null?xc(e):Te=i}function D0(e){var i=e,s=i.alternate;switch(i.tag){case 15:case 0:i=Yg(s,i,i.pendingProps,i.type,void 0,we);break;case 11:i=Yg(s,i,i.pendingProps,i.type.render,i.ref,we);break;case 5:Cf(i);default:n0(s,i),i=Te=Cm(i,va),i=t0(s,i,va)}e.memoizedProps=e.pendingProps,i===null?xc(e):Te=i}function Rr(e,i,s,o){la=Cs=null,Cf(i),_r=null,Co=0;var u=i.return;try{if(oS(e,u,i,s,we)){un=1,rc(e,_i(s,e.current)),Te=null;return}}catch(p){if(u!==null)throw Te=u,p;un=1,rc(e,_i(s,e.current)),Te=null;return}i.flags&32768?(Ce||o===1?e=!0:Er||(we&536870912)!==0?e=!1:(Qa=e=!0,(o===2||o===9||o===3||o===6)&&(o=ri.current,o!==null&&o.tag===13&&(o.flags|=16384))),U0(i,e)):xc(i)}function xc(e){var i=e;do{if((i.flags&32768)!==0){U0(i,Qa);return}e=i.return;var s=uS(i.alternate,i,va);if(s!==null){Te=s;return}if(i=i.sibling,i!==null){Te=i;return}Te=i=e}while(i!==null);un===0&&(un=5)}function U0(e,i){do{var s=fS(e.alternate,e);if(s!==null){s.flags&=32767,Te=s;return}if(s=e.return,s!==null&&(s.flags|=32768,s.subtreeFlags=0,s.deletions=null),!i&&(e=e.sibling,e!==null)){Te=e;return}Te=e=s}while(e!==null);un=6,Te=null}function L0(e,i,s,o,u,p,M,N,H){e.cancelPendingCommit=null;do yc();while(bn!==0);if((ze&6)!==0)throw Error(a(327));if(i!==null){if(i===e.current)throw Error(a(177));if(p=i.lanes|i.childLanes,p|=ef,ln(e,s,p,M,N,H),e===Je&&(Te=Je=null,we=0),Ar=i,ts=e,xa=s,ud=p,fd=u,M0=o,(i.subtreeFlags&10256)!==0||(i.flags&10256)!==0?(e.callbackNode=null,e.callbackPriority=0,bS(tt,function(){return B0(),null})):(e.callbackNode=null,e.callbackPriority=0),o=(i.flags&13878)!==0,(i.subtreeFlags&13878)!==0||o){o=P.T,P.T=null,u=F.p,F.p=2,M=ze,ze|=4;try{dS(e,i,s)}finally{ze=M,F.p=u,P.T=o}}bn=1,O0(),P0(),I0()}}function O0(){if(bn===1){bn=0;var e=ts,i=Ar,s=(i.flags&13878)!==0;if((i.subtreeFlags&13878)!==0||s){s=P.T,P.T=null;var o=F.p;F.p=2;var u=ze;ze|=4;try{p0(i,e);var p=Ad,M=ym(e.containerInfo),N=p.focusedElem,H=p.selectionRange;if(M!==N&&N&&N.ownerDocument&&xm(N.ownerDocument.documentElement,N)){if(H!==null&&Ku(N)){var nt=H.start,mt=H.end;if(mt===void 0&&(mt=nt),"selectionStart"in N)N.selectionStart=nt,N.selectionEnd=Math.min(mt,N.value.length);else{var St=N.ownerDocument||document,lt=St&&St.defaultView||window;if(lt.getSelection){var ft=lt.getSelection(),Qt=N.textContent.length,oe=Math.min(H.start,Qt),Ke=H.end===void 0?oe:Math.min(H.end,Qt);!ft.extend&&oe>Ke&&(M=Ke,Ke=oe,oe=M);var Z=vm(N,oe),X=vm(N,Ke);if(Z&&X&&(ft.rangeCount!==1||ft.anchorNode!==Z.node||ft.anchorOffset!==Z.offset||ft.focusNode!==X.node||ft.focusOffset!==X.offset)){var et=St.createRange();et.setStart(Z.node,Z.offset),ft.removeAllRanges(),oe>Ke?(ft.addRange(et),ft.extend(X.node,X.offset)):(et.setEnd(X.node,X.offset),ft.addRange(et))}}}}for(St=[],ft=N;ft=ft.parentNode;)ft.nodeType===1&&St.push({element:ft,left:ft.scrollLeft,top:ft.scrollTop});for(typeof N.focus=="function"&&N.focus(),N=0;N<St.length;N++){var vt=St[N];vt.element.scrollLeft=vt.left,vt.element.scrollTop=vt.top}}Uc=!!Td,Ad=Td=null}finally{ze=u,F.p=o,P.T=s}}e.current=i,bn=2}}function P0(){if(bn===2){bn=0;var e=ts,i=Ar,s=(i.flags&8772)!==0;if((i.subtreeFlags&8772)!==0||s){s=P.T,P.T=null;var o=F.p;F.p=2;var u=ze;ze|=4;try{c0(e,i.alternate,i)}finally{ze=u,F.p=o,P.T=s}}bn=3}}function I0(){if(bn===4||bn===3){bn=0,qe();var e=ts,i=Ar,s=xa,o=M0;(i.subtreeFlags&10256)!==0||(i.flags&10256)!==0?bn=5:(bn=0,Ar=ts=null,z0(e,e.pendingLanes));var u=e.pendingLanes;if(u===0&&($a=null),uo(s),i=i.stateNode,pt&&typeof pt.onCommitFiberRoot=="function")try{pt.onCommitFiberRoot(dt,i,void 0,(i.current.flags&128)===128)}catch{}if(o!==null){i=P.T,u=F.p,F.p=2,P.T=null;try{for(var p=e.onRecoverableError,M=0;M<o.length;M++){var N=o[M];p(N.value,{componentStack:N.stack})}}finally{P.T=i,F.p=u}}(xa&3)!==0&&yc(),Xi(e),u=e.pendingLanes,(s&261930)!==0&&(u&42)!==0?e===dd?Wo++:(Wo=0,dd=e):Wo=0,qo(0)}}function z0(e,i){(e.pooledCacheLanes&=i)===0&&(i=e.pooledCache,i!=null&&(e.pooledCache=null,wo(i)))}function yc(){return O0(),P0(),I0(),B0()}function B0(){if(bn!==5)return!1;var e=ts,i=ud;ud=0;var s=uo(xa),o=P.T,u=F.p;try{F.p=32>s?32:s,P.T=null,s=fd,fd=null;var p=ts,M=xa;if(bn=0,Ar=ts=null,xa=0,(ze&6)!==0)throw Error(a(331));var N=ze;if(ze|=4,x0(p.current),g0(p,p.current,M,s),ze=N,qo(0,!1),pt&&typeof pt.onPostCommitFiberRoot=="function")try{pt.onPostCommitFiberRoot(dt,p)}catch{}return!0}finally{F.p=u,P.T=o,z0(e,i)}}function F0(e,i,s){i=_i(s,i),i=jf(e.stateNode,i,2),e=qa(e,i,2),e!==null&&(ue(e,2),Xi(e))}function ke(e,i,s){if(e.tag===3)F0(e,e,s);else for(;i!==null;){if(i.tag===3){F0(i,e,s);break}else if(i.tag===1){var o=i.stateNode;if(typeof i.type.getDerivedStateFromError=="function"||typeof o.componentDidCatch=="function"&&($a===null||!$a.has(o))){e=_i(s,e),s=Hg(2),o=qa(i,s,2),o!==null&&(Gg(s,o,i,e),ue(o,2),Xi(o));break}}i=i.return}}function md(e,i,s){var o=e.pingCache;if(o===null){o=e.pingCache=new mS;var u=new Set;o.set(i,u)}else u=o.get(i),u===void 0&&(u=new Set,o.set(i,u));u.has(s)||(od=!0,u.add(s),e=yS.bind(null,e,i,s),i.then(e,e))}function yS(e,i,s){var o=e.pingCache;o!==null&&o.delete(i),e.pingedLanes|=e.suspendedLanes&s,e.warmLanes&=~s,Je===e&&(we&s)===s&&(un===4||un===3&&(we&62914560)===we&&300>Dt()-pc?(ze&2)===0&&wr(e,0):ld|=s,Tr===we&&(Tr=0)),Xi(e)}function H0(e,i){i===0&&(i=Et()),e=As(e,i),e!==null&&(ue(e,i),Xi(e))}function SS(e){var i=e.memoizedState,s=0;i!==null&&(s=i.retryLane),H0(e,s)}function MS(e,i){var s=0;switch(e.tag){case 31:case 13:var o=e.stateNode,u=e.memoizedState;u!==null&&(s=u.retryLane);break;case 19:o=e.stateNode;break;case 22:o=e.stateNode._retryCache;break;default:throw Error(a(314))}o!==null&&o.delete(i),H0(e,s)}function bS(e,i){return W(e,i)}var Sc=null,Cr=null,gd=!1,Mc=!1,_d=!1,ns=0;function Xi(e){e!==Cr&&e.next===null&&(Cr===null?Sc=Cr=e:Cr=Cr.next=e),Mc=!0,gd||(gd=!0,TS())}function qo(e,i){if(!_d&&Mc){_d=!0;do for(var s=!1,o=Sc;o!==null;){if(e!==0){var u=o.pendingLanes;if(u===0)var p=0;else{var M=o.suspendedLanes,N=o.pingedLanes;p=(1<<31-Ft(42|e)+1)-1,p&=u&~(M&~N),p=p&201326741?p&201326741|1:p?p|2:0}p!==0&&(s=!0,j0(o,p))}else p=we,p=_t(o,o===Je?p:0,o.cancelPendingCommit!==null||o.timeoutHandle!==-1),(p&3)===0||jt(o,p)||(s=!0,j0(o,p));o=o.next}while(s);_d=!1}}function ES(){G0()}function G0(){Mc=gd=!1;var e=0;ns!==0&&PS()&&(e=ns);for(var i=Dt(),s=null,o=Sc;o!==null;){var u=o.next,p=V0(o,i);p===0?(o.next=null,s===null?Sc=u:s.next=u,u===null&&(Cr=s)):(s=o,(e!==0||(p&3)!==0)&&(Mc=!0)),o=u}bn!==0&&bn!==5||qo(e),ns!==0&&(ns=0)}function V0(e,i){for(var s=e.suspendedLanes,o=e.pingedLanes,u=e.expirationTimes,p=e.pendingLanes&-62914561;0<p;){var M=31-Ft(p),N=1<<M,H=u[M];H===-1?((N&s)===0||(N&o)!==0)&&(u[M]=It(N,i)):H<=i&&(e.expiredLanes|=N),p&=~N}if(i=Je,s=we,s=_t(e,e===i?s:0,e.cancelPendingCommit!==null||e.timeoutHandle!==-1),o=e.callbackNode,s===0||e===i&&(Ve===2||Ve===9)||e.cancelPendingCommit!==null)return o!==null&&o!==null&&We(o),e.callbackNode=null,e.callbackPriority=0;if((s&3)===0||jt(e,s)){if(i=s&-s,i===e.callbackPriority)return i;switch(o!==null&&We(o),uo(s)){case 2:case 8:s=T;break;case 32:s=tt;break;case 268435456:s=At;break;default:s=tt}return o=k0.bind(null,e),s=W(s,o),e.callbackPriority=i,e.callbackNode=s,i}return o!==null&&o!==null&&We(o),e.callbackPriority=2,e.callbackNode=null,2}function k0(e,i){if(bn!==0&&bn!==5)return e.callbackNode=null,e.callbackPriority=0,null;var s=e.callbackNode;if(yc()&&e.callbackNode!==s)return null;var o=we;return o=_t(e,e===Je?o:0,e.cancelPendingCommit!==null||e.timeoutHandle!==-1),o===0?null:(E0(e,o,i),V0(e,Dt()),e.callbackNode!=null&&e.callbackNode===s?k0.bind(null,e):null)}function j0(e,i){if(yc())return null;E0(e,i,!0)}function TS(){zS(function(){(ze&6)!==0?W(L,ES):G0()})}function vd(){if(ns===0){var e=pr;e===0&&(e=de,de<<=1,(de&261888)===0&&(de=256)),ns=e}return ns}function X0(e){return e==null||typeof e=="symbol"||typeof e=="boolean"?null:typeof e=="function"?e:Ms(""+e)}function W0(e,i){var s=i.ownerDocument.createElement("input");return s.name=i.name,s.value=i.value,e.id&&s.setAttribute("form",e.id),i.parentNode.insertBefore(s,i),e=new FormData(e),s.parentNode.removeChild(s),e}function AS(e,i,s,o,u){if(i==="submit"&&s&&s.stateNode===u){var p=X0((u[Cn]||null).action),M=o.submitter;M&&(i=(i=M[Cn]||null)?X0(i.formAction):M.getAttribute("formAction"),i!==null&&(p=i,M=null));var N=new Ol("action","action",null,o,u);e.push({event:N,listeners:[{instance:null,listener:function(){if(o.defaultPrevented){if(ns!==0){var H=M?W0(u,M):new FormData(u);Bf(s,{pending:!0,data:H,method:u.method,action:p},null,H)}}else typeof p=="function"&&(N.preventDefault(),H=M?W0(u,M):new FormData(u),Bf(s,{pending:!0,data:H,method:u.method,action:p},p,H))},currentTarget:u}]})}}for(var xd=0;xd<tf.length;xd++){var yd=tf[xd],wS=yd.toLowerCase(),RS=yd[0].toUpperCase()+yd.slice(1);Li(wS,"on"+RS)}Li(bm,"onAnimationEnd"),Li(Em,"onAnimationIteration"),Li(Tm,"onAnimationStart"),Li("dblclick","onDoubleClick"),Li("focusin","onFocus"),Li("focusout","onBlur"),Li(jy,"onTransitionRun"),Li(Xy,"onTransitionStart"),Li(Wy,"onTransitionCancel"),Li(Am,"onTransitionEnd"),ot("onMouseEnter",["mouseout","mouseover"]),ot("onMouseLeave",["mouseout","mouseover"]),ot("onPointerEnter",["pointerout","pointerover"]),ot("onPointerLeave",["pointerout","pointerover"]),Y("onChange","change click focusin focusout input keydown keyup selectionchange".split(" ")),Y("onSelect","focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")),Y("onBeforeInput",["compositionend","keypress","textInput","paste"]),Y("onCompositionEnd","compositionend focusout keydown keypress keyup mousedown".split(" ")),Y("onCompositionStart","compositionstart focusout keydown keypress keyup mousedown".split(" ")),Y("onCompositionUpdate","compositionupdate focusout keydown keypress keyup mousedown".split(" "));var Yo="abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "),CS=new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(Yo));function q0(e,i){i=(i&4)!==0;for(var s=0;s<e.length;s++){var o=e[s],u=o.event;o=o.listeners;t:{var p=void 0;if(i)for(var M=o.length-1;0<=M;M--){var N=o[M],H=N.instance,nt=N.currentTarget;if(N=N.listener,H!==p&&u.isPropagationStopped())break t;p=N,u.currentTarget=nt;try{p(u)}catch(mt){zl(mt)}u.currentTarget=null,p=H}else for(M=0;M<o.length;M++){if(N=o[M],H=N.instance,nt=N.currentTarget,N=N.listener,H!==p&&u.isPropagationStopped())break t;p=N,u.currentTarget=nt;try{p(u)}catch(mt){zl(mt)}u.currentTarget=null,p=H}}}}function Ae(e,i){var s=i[Oa];s===void 0&&(s=i[Oa]=new Set);var o=e+"__bubble";s.has(o)||(Y0(i,e,2,!1),s.add(o))}function Sd(e,i,s){var o=0;i&&(o|=4),Y0(s,e,o,i)}var bc="_reactListening"+Math.random().toString(36).slice(2);function Md(e){if(!e[bc]){e[bc]=!0,Nl.forEach(function(s){s!=="selectionchange"&&(CS.has(s)||Sd(s,!1,e),Sd(s,!0,e))});var i=e.nodeType===9?e:e.ownerDocument;i===null||i[bc]||(i[bc]=!0,Sd("selectionchange",!1,i))}}function Y0(e,i,s,o){switch(b_(i)){case 2:var u=iM;break;case 8:u=aM;break;default:u=zd}s=u.bind(null,i,s,e),u=void 0,!Gu||i!=="touchstart"&&i!=="touchmove"&&i!=="wheel"||(u=!0),o?u!==void 0?e.addEventListener(i,s,{capture:!0,passive:u}):e.addEventListener(i,s,!0):u!==void 0?e.addEventListener(i,s,{passive:u}):e.addEventListener(i,s,!1)}function bd(e,i,s,o,u){var p=o;if((i&1)===0&&(i&2)===0&&o!==null)t:for(;;){if(o===null)return;var M=o.tag;if(M===3||M===4){var N=o.stateNode.containerInfo;if(N===u)break;if(M===4)for(M=o.return;M!==null;){var H=M.tag;if((H===3||H===4)&&M.stateNode.containerInfo===u)return;M=M.return}for(;N!==null;){if(M=Ia(N),M===null)return;if(H=M.tag,H===5||H===6||H===26||H===27){o=p=M;continue t}N=N.parentNode}}o=o.return}$p(function(){var nt=p,mt=Fu(s),St=[];t:{var lt=wm.get(e);if(lt!==void 0){var ft=Ol,Qt=e;switch(e){case"keypress":if(Ul(s)===0)break t;case"keydown":case"keyup":ft=My;break;case"focusin":Qt="focus",ft=Xu;break;case"focusout":Qt="blur",ft=Xu;break;case"beforeblur":case"afterblur":ft=Xu;break;case"click":if(s.button===2)break t;case"auxclick":case"dblclick":case"mousedown":case"mousemove":case"mouseup":case"mouseout":case"mouseover":case"contextmenu":ft=nm;break;case"drag":case"dragend":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"dragstart":case"drop":ft=uy;break;case"touchcancel":case"touchend":case"touchmove":case"touchstart":ft=Ty;break;case bm:case Em:case Tm:ft=hy;break;case Am:ft=wy;break;case"scroll":case"scrollend":ft=ly;break;case"wheel":ft=Cy;break;case"copy":case"cut":case"paste":ft=my;break;case"gotpointercapture":case"lostpointercapture":case"pointercancel":case"pointerdown":case"pointermove":case"pointerout":case"pointerover":case"pointerup":ft=am;break;case"toggle":case"beforetoggle":ft=Dy}var oe=(i&4)!==0,Ke=!oe&&(e==="scroll"||e==="scrollend"),Z=oe?lt!==null?lt+"Capture":null:lt;oe=[];for(var X=nt,et;X!==null;){var vt=X;if(et=vt.stateNode,vt=vt.tag,vt!==5&&vt!==26&&vt!==27||et===null||Z===null||(vt=go(X,Z),vt!=null&&oe.push(Zo(X,vt,et))),Ke)break;X=X.return}0<oe.length&&(lt=new ft(lt,Qt,null,s,mt),St.push({event:lt,listeners:oe}))}}if((i&7)===0){t:{if(lt=e==="mouseover"||e==="pointerover",ft=e==="mouseout"||e==="pointerout",lt&&s!==Bu&&(Qt=s.relatedTarget||s.fromElement)&&(Ia(Qt)||Qt[ia]))break t;if((ft||lt)&&(lt=mt.window===mt?mt:(lt=mt.ownerDocument)?lt.defaultView||lt.parentWindow:window,ft?(Qt=s.relatedTarget||s.toElement,ft=nt,Qt=Qt?Ia(Qt):null,Qt!==null&&(Ke=c(Qt),oe=Qt.tag,Qt!==Ke||oe!==5&&oe!==27&&oe!==6)&&(Qt=null)):(ft=null,Qt=nt),ft!==Qt)){if(oe=nm,vt="onMouseLeave",Z="onMouseEnter",X="mouse",(e==="pointerout"||e==="pointerover")&&(oe=am,vt="onPointerLeave",Z="onPointerEnter",X="pointer"),Ke=ft==null?lt:Ss(ft),et=Qt==null?lt:Ss(Qt),lt=new oe(vt,X+"leave",ft,s,mt),lt.target=Ke,lt.relatedTarget=et,vt=null,Ia(mt)===nt&&(oe=new oe(Z,X+"enter",Qt,s,mt),oe.target=et,oe.relatedTarget=Ke,vt=oe),Ke=vt,ft&&Qt)e:{for(oe=NS,Z=ft,X=Qt,et=0,vt=Z;vt;vt=oe(vt))et++;vt=0;for(var se=X;se;se=oe(se))vt++;for(;0<et-vt;)Z=oe(Z),et--;for(;0<vt-et;)X=oe(X),vt--;for(;et--;){if(Z===X||X!==null&&Z===X.alternate){oe=Z;break e}Z=oe(Z),X=oe(X)}oe=null}else oe=null;ft!==null&&Z0(St,lt,ft,oe,!1),Qt!==null&&Ke!==null&&Z0(St,Ke,Qt,oe,!0)}}t:{if(lt=nt?Ss(nt):window,ft=lt.nodeName&&lt.nodeName.toLowerCase(),ft==="select"||ft==="input"&&lt.type==="file")var Oe=dm;else if(um(lt))if(hm)Oe=Gy;else{Oe=Fy;var $t=By}else ft=lt.nodeName,!ft||ft.toLowerCase()!=="input"||lt.type!=="checkbox"&&lt.type!=="radio"?nt&&Be(nt.elementType)&&(Oe=dm):Oe=Hy;if(Oe&&(Oe=Oe(e,nt))){fm(St,Oe,s,mt);break t}$t&&$t(e,lt,nt),e==="focusout"&&nt&&lt.type==="number"&&nt.memoizedProps.value!=null&&Ee(lt,"number",lt.value)}switch($t=nt?Ss(nt):window,e){case"focusin":(um($t)||$t.contentEditable==="true")&&(rr=$t,Qu=nt,Eo=null);break;case"focusout":Eo=Qu=rr=null;break;case"mousedown":Ju=!0;break;case"contextmenu":case"mouseup":case"dragend":Ju=!1,Sm(St,s,mt);break;case"selectionchange":if(ky)break;case"keydown":case"keyup":Sm(St,s,mt)}var ve;if(qu)t:{switch(e){case"compositionstart":var Re="onCompositionStart";break t;case"compositionend":Re="onCompositionEnd";break t;case"compositionupdate":Re="onCompositionUpdate";break t}Re=void 0}else sr?lm(e,s)&&(Re="onCompositionEnd"):e==="keydown"&&s.keyCode===229&&(Re="onCompositionStart");Re&&(sm&&s.locale!=="ko"&&(sr||Re!=="onCompositionStart"?Re==="onCompositionEnd"&&sr&&(ve=tm()):(Ha=mt,Vu="value"in Ha?Ha.value:Ha.textContent,sr=!0)),$t=Ec(nt,Re),0<$t.length&&(Re=new im(Re,e,null,s,mt),St.push({event:Re,listeners:$t}),ve?Re.data=ve:(ve=cm(s),ve!==null&&(Re.data=ve)))),(ve=Ly?Oy(e,s):Py(e,s))&&(Re=Ec(nt,"onBeforeInput"),0<Re.length&&($t=new im("onBeforeInput","beforeinput",null,s,mt),St.push({event:$t,listeners:Re}),$t.data=ve)),AS(St,e,nt,s,mt)}q0(St,i)})}function Zo(e,i,s){return{instance:e,listener:i,currentTarget:s}}function Ec(e,i){for(var s=i+"Capture",o=[];e!==null;){var u=e,p=u.stateNode;if(u=u.tag,u!==5&&u!==26&&u!==27||p===null||(u=go(e,s),u!=null&&o.unshift(Zo(e,u,p)),u=go(e,i),u!=null&&o.push(Zo(e,u,p))),e.tag===3)return o;e=e.return}return[]}function NS(e){if(e===null)return null;do e=e.return;while(e&&e.tag!==5&&e.tag!==27);return e||null}function Z0(e,i,s,o,u){for(var p=i._reactName,M=[];s!==null&&s!==o;){var N=s,H=N.alternate,nt=N.stateNode;if(N=N.tag,H!==null&&H===o)break;N!==5&&N!==26&&N!==27||nt===null||(H=nt,u?(nt=go(s,p),nt!=null&&M.unshift(Zo(s,nt,H))):u||(nt=go(s,p),nt!=null&&M.push(Zo(s,nt,H)))),s=s.return}M.length!==0&&e.push({event:i,listeners:M})}var DS=/\r\n?/g,US=/\u0000|\uFFFD/g;function K0(e){return(typeof e=="string"?e:""+e).replace(DS,`
`).replace(US,"")}function Q0(e,i){return i=K0(i),K0(e)===i}function Ze(e,i,s,o,u,p){switch(s){case"children":typeof o=="string"?i==="body"||i==="textarea"&&o===""||ii(e,o):(typeof o=="number"||typeof o=="bigint")&&i!=="body"&&ii(e,""+o);break;case"className":Kt(e,"class",o);break;case"tabIndex":Kt(e,"tabindex",o);break;case"dir":case"role":case"viewBox":case"width":case"height":Kt(e,s,o);break;case"style":Ui(e,o,p);break;case"data":if(i!=="object"){Kt(e,"data",o);break}case"src":case"href":if(o===""&&(i!=="a"||s!=="href")){e.removeAttribute(s);break}if(o==null||typeof o=="function"||typeof o=="symbol"||typeof o=="boolean"){e.removeAttribute(s);break}o=Ms(""+o),e.setAttribute(s,o);break;case"action":case"formAction":if(typeof o=="function"){e.setAttribute(s,"javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");break}else typeof p=="function"&&(s==="formAction"?(i!=="input"&&Ze(e,i,"name",u.name,u,null),Ze(e,i,"formEncType",u.formEncType,u,null),Ze(e,i,"formMethod",u.formMethod,u,null),Ze(e,i,"formTarget",u.formTarget,u,null)):(Ze(e,i,"encType",u.encType,u,null),Ze(e,i,"method",u.method,u,null),Ze(e,i,"target",u.target,u,null)));if(o==null||typeof o=="symbol"||typeof o=="boolean"){e.removeAttribute(s);break}o=Ms(""+o),e.setAttribute(s,o);break;case"onClick":o!=null&&(e.onclick=aa);break;case"onScroll":o!=null&&Ae("scroll",e);break;case"onScrollEnd":o!=null&&Ae("scrollend",e);break;case"dangerouslySetInnerHTML":if(o!=null){if(typeof o!="object"||!("__html"in o))throw Error(a(61));if(s=o.__html,s!=null){if(u.children!=null)throw Error(a(60));e.innerHTML=s}}break;case"multiple":e.multiple=o&&typeof o!="function"&&typeof o!="symbol";break;case"muted":e.muted=o&&typeof o!="function"&&typeof o!="symbol";break;case"suppressContentEditableWarning":case"suppressHydrationWarning":case"defaultValue":case"defaultChecked":case"innerHTML":case"ref":break;case"autoFocus":break;case"xlinkHref":if(o==null||typeof o=="function"||typeof o=="boolean"||typeof o=="symbol"){e.removeAttribute("xlink:href");break}s=Ms(""+o),e.setAttributeNS("http://www.w3.org/1999/xlink","xlink:href",s);break;case"contentEditable":case"spellCheck":case"draggable":case"value":case"autoReverse":case"externalResourcesRequired":case"focusable":case"preserveAlpha":o!=null&&typeof o!="function"&&typeof o!="symbol"?e.setAttribute(s,""+o):e.removeAttribute(s);break;case"inert":case"allowFullScreen":case"async":case"autoPlay":case"controls":case"default":case"defer":case"disabled":case"disablePictureInPicture":case"disableRemotePlayback":case"formNoValidate":case"hidden":case"loop":case"noModule":case"noValidate":case"open":case"playsInline":case"readOnly":case"required":case"reversed":case"scoped":case"seamless":case"itemScope":o&&typeof o!="function"&&typeof o!="symbol"?e.setAttribute(s,""):e.removeAttribute(s);break;case"capture":case"download":o===!0?e.setAttribute(s,""):o!==!1&&o!=null&&typeof o!="function"&&typeof o!="symbol"?e.setAttribute(s,o):e.removeAttribute(s);break;case"cols":case"rows":case"size":case"span":o!=null&&typeof o!="function"&&typeof o!="symbol"&&!isNaN(o)&&1<=o?e.setAttribute(s,o):e.removeAttribute(s);break;case"rowSpan":case"start":o==null||typeof o=="function"||typeof o=="symbol"||isNaN(o)?e.removeAttribute(s):e.setAttribute(s,o);break;case"popover":Ae("beforetoggle",e),Ae("toggle",e),Vt(e,"popover",o);break;case"xlinkActuate":Zt(e,"http://www.w3.org/1999/xlink","xlink:actuate",o);break;case"xlinkArcrole":Zt(e,"http://www.w3.org/1999/xlink","xlink:arcrole",o);break;case"xlinkRole":Zt(e,"http://www.w3.org/1999/xlink","xlink:role",o);break;case"xlinkShow":Zt(e,"http://www.w3.org/1999/xlink","xlink:show",o);break;case"xlinkTitle":Zt(e,"http://www.w3.org/1999/xlink","xlink:title",o);break;case"xlinkType":Zt(e,"http://www.w3.org/1999/xlink","xlink:type",o);break;case"xmlBase":Zt(e,"http://www.w3.org/XML/1998/namespace","xml:base",o);break;case"xmlLang":Zt(e,"http://www.w3.org/XML/1998/namespace","xml:lang",o);break;case"xmlSpace":Zt(e,"http://www.w3.org/XML/1998/namespace","xml:space",o);break;case"is":Vt(e,"is",o);break;case"innerText":case"textContent":break;default:(!(2<s.length)||s[0]!=="o"&&s[0]!=="O"||s[1]!=="n"&&s[1]!=="N")&&(s=Gi.get(s)||s,Vt(e,s,o))}}function Ed(e,i,s,o,u,p){switch(s){case"style":Ui(e,o,p);break;case"dangerouslySetInnerHTML":if(o!=null){if(typeof o!="object"||!("__html"in o))throw Error(a(61));if(s=o.__html,s!=null){if(u.children!=null)throw Error(a(60));e.innerHTML=s}}break;case"children":typeof o=="string"?ii(e,o):(typeof o=="number"||typeof o=="bigint")&&ii(e,""+o);break;case"onScroll":o!=null&&Ae("scroll",e);break;case"onScrollEnd":o!=null&&Ae("scrollend",e);break;case"onClick":o!=null&&(e.onclick=aa);break;case"suppressContentEditableWarning":case"suppressHydrationWarning":case"innerHTML":case"ref":break;case"innerText":case"textContent":break;default:if(!C.hasOwnProperty(s))t:{if(s[0]==="o"&&s[1]==="n"&&(u=s.endsWith("Capture"),i=s.slice(2,u?s.length-7:void 0),p=e[Cn]||null,p=p!=null?p[s]:null,typeof p=="function"&&e.removeEventListener(i,p,u),typeof o=="function")){typeof p!="function"&&p!==null&&(s in e?e[s]=null:e.hasAttribute(s)&&e.removeAttribute(s)),e.addEventListener(i,o,u);break t}s in e?e[s]=o:o===!0?e.setAttribute(s,""):Vt(e,s,o)}}}function Ln(e,i,s){switch(i){case"div":case"span":case"svg":case"path":case"a":case"g":case"p":case"li":break;case"img":Ae("error",e),Ae("load",e);var o=!1,u=!1,p;for(p in s)if(s.hasOwnProperty(p)){var M=s[p];if(M!=null)switch(p){case"src":o=!0;break;case"srcSet":u=!0;break;case"children":case"dangerouslySetInnerHTML":throw Error(a(137,i));default:Ze(e,i,p,M,s,null)}}u&&Ze(e,i,"srcSet",s.srcSet,s,null),o&&Ze(e,i,"src",s.src,s,null);return;case"input":Ae("invalid",e);var N=p=M=u=null,H=null,nt=null;for(o in s)if(s.hasOwnProperty(o)){var mt=s[o];if(mt!=null)switch(o){case"name":u=mt;break;case"type":M=mt;break;case"checked":H=mt;break;case"defaultChecked":nt=mt;break;case"value":p=mt;break;case"defaultValue":N=mt;break;case"children":case"dangerouslySetInnerHTML":if(mt!=null)throw Error(a(137,i));break;default:Ze(e,i,o,mt,s,null)}}zn(e,p,N,H,nt,M,u,!1);return;case"select":Ae("invalid",e),o=M=p=null;for(u in s)if(s.hasOwnProperty(u)&&(N=s[u],N!=null))switch(u){case"value":p=N;break;case"defaultValue":M=N;break;case"multiple":o=N;default:Ze(e,i,u,N,s,null)}i=p,s=M,e.multiple=!!o,i!=null?Mn(e,!!o,i,!1):s!=null&&Mn(e,!!o,s,!0);return;case"textarea":Ae("invalid",e),p=u=o=null;for(M in s)if(s.hasOwnProperty(M)&&(N=s[M],N!=null))switch(M){case"value":o=N;break;case"defaultValue":u=N;break;case"children":p=N;break;case"dangerouslySetInnerHTML":if(N!=null)throw Error(a(91));break;default:Ze(e,i,M,N,s,null)}Di(e,o,u,p);return;case"option":for(H in s)if(s.hasOwnProperty(H)&&(o=s[H],o!=null))switch(H){case"selected":e.selected=o&&typeof o!="function"&&typeof o!="symbol";break;default:Ze(e,i,H,o,s,null)}return;case"dialog":Ae("beforetoggle",e),Ae("toggle",e),Ae("cancel",e),Ae("close",e);break;case"iframe":case"object":Ae("load",e);break;case"video":case"audio":for(o=0;o<Yo.length;o++)Ae(Yo[o],e);break;case"image":Ae("error",e),Ae("load",e);break;case"details":Ae("toggle",e);break;case"embed":case"source":case"link":Ae("error",e),Ae("load",e);case"area":case"base":case"br":case"col":case"hr":case"keygen":case"meta":case"param":case"track":case"wbr":case"menuitem":for(nt in s)if(s.hasOwnProperty(nt)&&(o=s[nt],o!=null))switch(nt){case"children":case"dangerouslySetInnerHTML":throw Error(a(137,i));default:Ze(e,i,nt,o,s,null)}return;default:if(Be(i)){for(mt in s)s.hasOwnProperty(mt)&&(o=s[mt],o!==void 0&&Ed(e,i,mt,o,s,void 0));return}}for(N in s)s.hasOwnProperty(N)&&(o=s[N],o!=null&&Ze(e,i,N,o,s,null))}function LS(e,i,s,o){switch(i){case"div":case"span":case"svg":case"path":case"a":case"g":case"p":case"li":break;case"input":var u=null,p=null,M=null,N=null,H=null,nt=null,mt=null;for(ft in s){var St=s[ft];if(s.hasOwnProperty(ft)&&St!=null)switch(ft){case"checked":break;case"value":break;case"defaultValue":H=St;default:o.hasOwnProperty(ft)||Ze(e,i,ft,null,o,St)}}for(var lt in o){var ft=o[lt];if(St=s[lt],o.hasOwnProperty(lt)&&(ft!=null||St!=null))switch(lt){case"type":p=ft;break;case"name":u=ft;break;case"checked":nt=ft;break;case"defaultChecked":mt=ft;break;case"value":M=ft;break;case"defaultValue":N=ft;break;case"children":case"dangerouslySetInnerHTML":if(ft!=null)throw Error(a(137,i));break;default:ft!==St&&Ze(e,i,lt,ft,o,St)}}qt(e,M,N,H,nt,mt,p,u);return;case"select":ft=M=N=lt=null;for(p in s)if(H=s[p],s.hasOwnProperty(p)&&H!=null)switch(p){case"value":break;case"multiple":ft=H;default:o.hasOwnProperty(p)||Ze(e,i,p,null,o,H)}for(u in o)if(p=o[u],H=s[u],o.hasOwnProperty(u)&&(p!=null||H!=null))switch(u){case"value":lt=p;break;case"defaultValue":N=p;break;case"multiple":M=p;default:p!==H&&Ze(e,i,u,p,o,H)}i=N,s=M,o=ft,lt!=null?Mn(e,!!s,lt,!1):!!o!=!!s&&(i!=null?Mn(e,!!s,i,!0):Mn(e,!!s,s?[]:"",!1));return;case"textarea":ft=lt=null;for(N in s)if(u=s[N],s.hasOwnProperty(N)&&u!=null&&!o.hasOwnProperty(N))switch(N){case"value":break;case"children":break;default:Ze(e,i,N,null,o,u)}for(M in o)if(u=o[M],p=s[M],o.hasOwnProperty(M)&&(u!=null||p!=null))switch(M){case"value":lt=u;break;case"defaultValue":ft=u;break;case"children":break;case"dangerouslySetInnerHTML":if(u!=null)throw Error(a(91));break;default:u!==p&&Ze(e,i,M,u,o,p)}ni(e,lt,ft);return;case"option":for(var Qt in s)if(lt=s[Qt],s.hasOwnProperty(Qt)&&lt!=null&&!o.hasOwnProperty(Qt))switch(Qt){case"selected":e.selected=!1;break;default:Ze(e,i,Qt,null,o,lt)}for(H in o)if(lt=o[H],ft=s[H],o.hasOwnProperty(H)&&lt!==ft&&(lt!=null||ft!=null))switch(H){case"selected":e.selected=lt&&typeof lt!="function"&&typeof lt!="symbol";break;default:Ze(e,i,H,lt,o,ft)}return;case"img":case"link":case"area":case"base":case"br":case"col":case"embed":case"hr":case"keygen":case"meta":case"param":case"source":case"track":case"wbr":case"menuitem":for(var oe in s)lt=s[oe],s.hasOwnProperty(oe)&&lt!=null&&!o.hasOwnProperty(oe)&&Ze(e,i,oe,null,o,lt);for(nt in o)if(lt=o[nt],ft=s[nt],o.hasOwnProperty(nt)&&lt!==ft&&(lt!=null||ft!=null))switch(nt){case"children":case"dangerouslySetInnerHTML":if(lt!=null)throw Error(a(137,i));break;default:Ze(e,i,nt,lt,o,ft)}return;default:if(Be(i)){for(var Ke in s)lt=s[Ke],s.hasOwnProperty(Ke)&&lt!==void 0&&!o.hasOwnProperty(Ke)&&Ed(e,i,Ke,void 0,o,lt);for(mt in o)lt=o[mt],ft=s[mt],!o.hasOwnProperty(mt)||lt===ft||lt===void 0&&ft===void 0||Ed(e,i,mt,lt,o,ft);return}}for(var Z in s)lt=s[Z],s.hasOwnProperty(Z)&&lt!=null&&!o.hasOwnProperty(Z)&&Ze(e,i,Z,null,o,lt);for(St in o)lt=o[St],ft=s[St],!o.hasOwnProperty(St)||lt===ft||lt==null&&ft==null||Ze(e,i,St,lt,o,ft)}function J0(e){switch(e){case"css":case"script":case"font":case"img":case"image":case"input":case"link":return!0;default:return!1}}function OS(){if(typeof performance.getEntriesByType=="function"){for(var e=0,i=0,s=performance.getEntriesByType("resource"),o=0;o<s.length;o++){var u=s[o],p=u.transferSize,M=u.initiatorType,N=u.duration;if(p&&N&&J0(M)){for(M=0,N=u.responseEnd,o+=1;o<s.length;o++){var H=s[o],nt=H.startTime;if(nt>N)break;var mt=H.transferSize,St=H.initiatorType;mt&&J0(St)&&(H=H.responseEnd,M+=mt*(H<N?1:(N-nt)/(H-nt)))}if(--o,i+=8*(p+M)/(u.duration/1e3),e++,10<e)break}}if(0<e)return i/e/1e6}return navigator.connection&&(e=navigator.connection.downlink,typeof e=="number")?e:5}var Td=null,Ad=null;function Tc(e){return e.nodeType===9?e:e.ownerDocument}function $0(e){switch(e){case"http://www.w3.org/2000/svg":return 1;case"http://www.w3.org/1998/Math/MathML":return 2;default:return 0}}function t_(e,i){if(e===0)switch(i){case"svg":return 1;case"math":return 2;default:return 0}return e===1&&i==="foreignObject"?0:e}function wd(e,i){return e==="textarea"||e==="noscript"||typeof i.children=="string"||typeof i.children=="number"||typeof i.children=="bigint"||typeof i.dangerouslySetInnerHTML=="object"&&i.dangerouslySetInnerHTML!==null&&i.dangerouslySetInnerHTML.__html!=null}var Rd=null;function PS(){var e=window.event;return e&&e.type==="popstate"?e===Rd?!1:(Rd=e,!0):(Rd=null,!1)}var e_=typeof setTimeout=="function"?setTimeout:void 0,IS=typeof clearTimeout=="function"?clearTimeout:void 0,n_=typeof Promise=="function"?Promise:void 0,zS=typeof queueMicrotask=="function"?queueMicrotask:typeof n_<"u"?function(e){return n_.resolve(null).then(e).catch(BS)}:e_;function BS(e){setTimeout(function(){throw e})}function is(e){return e==="head"}function i_(e,i){var s=i,o=0;do{var u=s.nextSibling;if(e.removeChild(s),u&&u.nodeType===8)if(s=u.data,s==="/$"||s==="/&"){if(o===0){e.removeChild(u),Lr(i);return}o--}else if(s==="$"||s==="$?"||s==="$~"||s==="$!"||s==="&")o++;else if(s==="html")Ko(e.ownerDocument.documentElement);else if(s==="head"){s=e.ownerDocument.head,Ko(s);for(var p=s.firstChild;p;){var M=p.nextSibling,N=p.nodeName;p[Pa]||N==="SCRIPT"||N==="STYLE"||N==="LINK"&&p.rel.toLowerCase()==="stylesheet"||s.removeChild(p),p=M}}else s==="body"&&Ko(e.ownerDocument.body);s=u}while(s);Lr(i)}function a_(e,i){var s=e;e=0;do{var o=s.nextSibling;if(s.nodeType===1?i?(s._stashedDisplay=s.style.display,s.style.display="none"):(s.style.display=s._stashedDisplay||"",s.getAttribute("style")===""&&s.removeAttribute("style")):s.nodeType===3&&(i?(s._stashedText=s.nodeValue,s.nodeValue=""):s.nodeValue=s._stashedText||""),o&&o.nodeType===8)if(s=o.data,s==="/$"){if(e===0)break;e--}else s!=="$"&&s!=="$?"&&s!=="$~"&&s!=="$!"||e++;s=o}while(s)}function Cd(e){var i=e.firstChild;for(i&&i.nodeType===10&&(i=i.nextSibling);i;){var s=i;switch(i=i.nextSibling,s.nodeName){case"HTML":case"HEAD":case"BODY":Cd(s),mo(s);continue;case"SCRIPT":case"STYLE":continue;case"LINK":if(s.rel.toLowerCase()==="stylesheet")continue}e.removeChild(s)}}function FS(e,i,s,o){for(;e.nodeType===1;){var u=s;if(e.nodeName.toLowerCase()!==i.toLowerCase()){if(!o&&(e.nodeName!=="INPUT"||e.type!=="hidden"))break}else if(o){if(!e[Pa])switch(i){case"meta":if(!e.hasAttribute("itemprop"))break;return e;case"link":if(p=e.getAttribute("rel"),p==="stylesheet"&&e.hasAttribute("data-precedence"))break;if(p!==u.rel||e.getAttribute("href")!==(u.href==null||u.href===""?null:u.href)||e.getAttribute("crossorigin")!==(u.crossOrigin==null?null:u.crossOrigin)||e.getAttribute("title")!==(u.title==null?null:u.title))break;return e;case"style":if(e.hasAttribute("data-precedence"))break;return e;case"script":if(p=e.getAttribute("src"),(p!==(u.src==null?null:u.src)||e.getAttribute("type")!==(u.type==null?null:u.type)||e.getAttribute("crossorigin")!==(u.crossOrigin==null?null:u.crossOrigin))&&p&&e.hasAttribute("async")&&!e.hasAttribute("itemprop"))break;return e;default:return e}}else if(i==="input"&&e.type==="hidden"){var p=u.name==null?null:""+u.name;if(u.type==="hidden"&&e.getAttribute("name")===p)return e}else return e;if(e=Mi(e.nextSibling),e===null)break}return null}function HS(e,i,s){if(i==="")return null;for(;e.nodeType!==3;)if((e.nodeType!==1||e.nodeName!=="INPUT"||e.type!=="hidden")&&!s||(e=Mi(e.nextSibling),e===null))return null;return e}function s_(e,i){for(;e.nodeType!==8;)if((e.nodeType!==1||e.nodeName!=="INPUT"||e.type!=="hidden")&&!i||(e=Mi(e.nextSibling),e===null))return null;return e}function Nd(e){return e.data==="$?"||e.data==="$~"}function Dd(e){return e.data==="$!"||e.data==="$?"&&e.ownerDocument.readyState!=="loading"}function GS(e,i){var s=e.ownerDocument;if(e.data==="$~")e._reactRetry=i;else if(e.data!=="$?"||s.readyState!=="loading")i();else{var o=function(){i(),s.removeEventListener("DOMContentLoaded",o)};s.addEventListener("DOMContentLoaded",o),e._reactRetry=o}}function Mi(e){for(;e!=null;e=e.nextSibling){var i=e.nodeType;if(i===1||i===3)break;if(i===8){if(i=e.data,i==="$"||i==="$!"||i==="$?"||i==="$~"||i==="&"||i==="F!"||i==="F")break;if(i==="/$"||i==="/&")return null}}return e}var Ud=null;function r_(e){e=e.nextSibling;for(var i=0;e;){if(e.nodeType===8){var s=e.data;if(s==="/$"||s==="/&"){if(i===0)return Mi(e.nextSibling);i--}else s!=="$"&&s!=="$!"&&s!=="$?"&&s!=="$~"&&s!=="&"||i++}e=e.nextSibling}return null}function o_(e){e=e.previousSibling;for(var i=0;e;){if(e.nodeType===8){var s=e.data;if(s==="$"||s==="$!"||s==="$?"||s==="$~"||s==="&"){if(i===0)return e;i--}else s!=="/$"&&s!=="/&"||i++}e=e.previousSibling}return null}function l_(e,i,s){switch(i=Tc(s),e){case"html":if(e=i.documentElement,!e)throw Error(a(452));return e;case"head":if(e=i.head,!e)throw Error(a(453));return e;case"body":if(e=i.body,!e)throw Error(a(454));return e;default:throw Error(a(451))}}function Ko(e){for(var i=e.attributes;i.length;)e.removeAttributeNode(i[0]);mo(e)}var bi=new Map,c_=new Set;function Ac(e){return typeof e.getRootNode=="function"?e.getRootNode():e.nodeType===9?e:e.ownerDocument}var ya=F.d;F.d={f:VS,r:kS,D:jS,C:XS,L:WS,m:qS,X:ZS,S:YS,M:KS};function VS(){var e=ya.f(),i=_c();return e||i}function kS(e){var i=za(e);i!==null&&i.tag===5&&i.type==="form"?Ag(i):ya.r(e)}var Nr=typeof document>"u"?null:document;function u_(e,i,s){var o=Nr;if(o&&typeof i=="string"&&i){var u=He(i);u='link[rel="'+e+'"][href="'+u+'"]',typeof s=="string"&&(u+='[crossorigin="'+s+'"]'),c_.has(u)||(c_.add(u),e={rel:e,crossOrigin:s,href:i},o.querySelector(u)===null&&(i=o.createElement("link"),Ln(i,"link",e),mn(i),o.head.appendChild(i)))}}function jS(e){ya.D(e),u_("dns-prefetch",e,null)}function XS(e,i){ya.C(e,i),u_("preconnect",e,i)}function WS(e,i,s){ya.L(e,i,s);var o=Nr;if(o&&e&&i){var u='link[rel="preload"][as="'+He(i)+'"]';i==="image"&&s&&s.imageSrcSet?(u+='[imagesrcset="'+He(s.imageSrcSet)+'"]',typeof s.imageSizes=="string"&&(u+='[imagesizes="'+He(s.imageSizes)+'"]')):u+='[href="'+He(e)+'"]';var p=u;switch(i){case"style":p=Dr(e);break;case"script":p=Ur(e)}bi.has(p)||(e=_({rel:"preload",href:i==="image"&&s&&s.imageSrcSet?void 0:e,as:i},s),bi.set(p,e),o.querySelector(u)!==null||i==="style"&&o.querySelector(Qo(p))||i==="script"&&o.querySelector(Jo(p))||(i=o.createElement("link"),Ln(i,"link",e),mn(i),o.head.appendChild(i)))}}function qS(e,i){ya.m(e,i);var s=Nr;if(s&&e){var o=i&&typeof i.as=="string"?i.as:"script",u='link[rel="modulepreload"][as="'+He(o)+'"][href="'+He(e)+'"]',p=u;switch(o){case"audioworklet":case"paintworklet":case"serviceworker":case"sharedworker":case"worker":case"script":p=Ur(e)}if(!bi.has(p)&&(e=_({rel:"modulepreload",href:e},i),bi.set(p,e),s.querySelector(u)===null)){switch(o){case"audioworklet":case"paintworklet":case"serviceworker":case"sharedworker":case"worker":case"script":if(s.querySelector(Jo(p)))return}o=s.createElement("link"),Ln(o,"link",e),mn(o),s.head.appendChild(o)}}}function YS(e,i,s){ya.S(e,i,s);var o=Nr;if(o&&e){var u=Ba(o).hoistableStyles,p=Dr(e);i=i||"default";var M=u.get(p);if(!M){var N={loading:0,preload:null};if(M=o.querySelector(Qo(p)))N.loading=5;else{e=_({rel:"stylesheet",href:e,"data-precedence":i},s),(s=bi.get(p))&&Ld(e,s);var H=M=o.createElement("link");mn(H),Ln(H,"link",e),H._p=new Promise(function(nt,mt){H.onload=nt,H.onerror=mt}),H.addEventListener("load",function(){N.loading|=1}),H.addEventListener("error",function(){N.loading|=2}),N.loading|=4,wc(M,i,o)}M={type:"stylesheet",instance:M,count:1,state:N},u.set(p,M)}}}function ZS(e,i){ya.X(e,i);var s=Nr;if(s&&e){var o=Ba(s).hoistableScripts,u=Ur(e),p=o.get(u);p||(p=s.querySelector(Jo(u)),p||(e=_({src:e,async:!0},i),(i=bi.get(u))&&Od(e,i),p=s.createElement("script"),mn(p),Ln(p,"link",e),s.head.appendChild(p)),p={type:"script",instance:p,count:1,state:null},o.set(u,p))}}function KS(e,i){ya.M(e,i);var s=Nr;if(s&&e){var o=Ba(s).hoistableScripts,u=Ur(e),p=o.get(u);p||(p=s.querySelector(Jo(u)),p||(e=_({src:e,async:!0,type:"module"},i),(i=bi.get(u))&&Od(e,i),p=s.createElement("script"),mn(p),Ln(p,"link",e),s.head.appendChild(p)),p={type:"script",instance:p,count:1,state:null},o.set(u,p))}}function f_(e,i,s,o){var u=(u=st.current)?Ac(u):null;if(!u)throw Error(a(446));switch(e){case"meta":case"title":return null;case"style":return typeof s.precedence=="string"&&typeof s.href=="string"?(i=Dr(s.href),s=Ba(u).hoistableStyles,o=s.get(i),o||(o={type:"style",instance:null,count:0,state:null},s.set(i,o)),o):{type:"void",instance:null,count:0,state:null};case"link":if(s.rel==="stylesheet"&&typeof s.href=="string"&&typeof s.precedence=="string"){e=Dr(s.href);var p=Ba(u).hoistableStyles,M=p.get(e);if(M||(u=u.ownerDocument||u,M={type:"stylesheet",instance:null,count:0,state:{loading:0,preload:null}},p.set(e,M),(p=u.querySelector(Qo(e)))&&!p._p&&(M.instance=p,M.state.loading=5),bi.has(e)||(s={rel:"preload",as:"style",href:s.href,crossOrigin:s.crossOrigin,integrity:s.integrity,media:s.media,hrefLang:s.hrefLang,referrerPolicy:s.referrerPolicy},bi.set(e,s),p||QS(u,e,s,M.state))),i&&o===null)throw Error(a(528,""));return M}if(i&&o!==null)throw Error(a(529,""));return null;case"script":return i=s.async,s=s.src,typeof s=="string"&&i&&typeof i!="function"&&typeof i!="symbol"?(i=Ur(s),s=Ba(u).hoistableScripts,o=s.get(i),o||(o={type:"script",instance:null,count:0,state:null},s.set(i,o)),o):{type:"void",instance:null,count:0,state:null};default:throw Error(a(444,e))}}function Dr(e){return'href="'+He(e)+'"'}function Qo(e){return'link[rel="stylesheet"]['+e+"]"}function d_(e){return _({},e,{"data-precedence":e.precedence,precedence:null})}function QS(e,i,s,o){e.querySelector('link[rel="preload"][as="style"]['+i+"]")?o.loading=1:(i=e.createElement("link"),o.preload=i,i.addEventListener("load",function(){return o.loading|=1}),i.addEventListener("error",function(){return o.loading|=2}),Ln(i,"link",s),mn(i),e.head.appendChild(i))}function Ur(e){return'[src="'+He(e)+'"]'}function Jo(e){return"script[async]"+e}function h_(e,i,s){if(i.count++,i.instance===null)switch(i.type){case"style":var o=e.querySelector('style[data-href~="'+He(s.href)+'"]');if(o)return i.instance=o,mn(o),o;var u=_({},s,{"data-href":s.href,"data-precedence":s.precedence,href:null,precedence:null});return o=(e.ownerDocument||e).createElement("style"),mn(o),Ln(o,"style",u),wc(o,s.precedence,e),i.instance=o;case"stylesheet":u=Dr(s.href);var p=e.querySelector(Qo(u));if(p)return i.state.loading|=4,i.instance=p,mn(p),p;o=d_(s),(u=bi.get(u))&&Ld(o,u),p=(e.ownerDocument||e).createElement("link"),mn(p);var M=p;return M._p=new Promise(function(N,H){M.onload=N,M.onerror=H}),Ln(p,"link",o),i.state.loading|=4,wc(p,s.precedence,e),i.instance=p;case"script":return p=Ur(s.src),(u=e.querySelector(Jo(p)))?(i.instance=u,mn(u),u):(o=s,(u=bi.get(p))&&(o=_({},s),Od(o,u)),e=e.ownerDocument||e,u=e.createElement("script"),mn(u),Ln(u,"link",o),e.head.appendChild(u),i.instance=u);case"void":return null;default:throw Error(a(443,i.type))}else i.type==="stylesheet"&&(i.state.loading&4)===0&&(o=i.instance,i.state.loading|=4,wc(o,s.precedence,e));return i.instance}function wc(e,i,s){for(var o=s.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'),u=o.length?o[o.length-1]:null,p=u,M=0;M<o.length;M++){var N=o[M];if(N.dataset.precedence===i)p=N;else if(p!==u)break}p?p.parentNode.insertBefore(e,p.nextSibling):(i=s.nodeType===9?s.head:s,i.insertBefore(e,i.firstChild))}function Ld(e,i){e.crossOrigin==null&&(e.crossOrigin=i.crossOrigin),e.referrerPolicy==null&&(e.referrerPolicy=i.referrerPolicy),e.title==null&&(e.title=i.title)}function Od(e,i){e.crossOrigin==null&&(e.crossOrigin=i.crossOrigin),e.referrerPolicy==null&&(e.referrerPolicy=i.referrerPolicy),e.integrity==null&&(e.integrity=i.integrity)}var Rc=null;function p_(e,i,s){if(Rc===null){var o=new Map,u=Rc=new Map;u.set(s,o)}else u=Rc,o=u.get(s),o||(o=new Map,u.set(s,o));if(o.has(e))return o;for(o.set(e,null),s=s.getElementsByTagName(e),u=0;u<s.length;u++){var p=s[u];if(!(p[Pa]||p[dn]||e==="link"&&p.getAttribute("rel")==="stylesheet")&&p.namespaceURI!=="http://www.w3.org/2000/svg"){var M=p.getAttribute(i)||"";M=e+M;var N=o.get(M);N?N.push(p):o.set(M,[p])}}return o}function m_(e,i,s){e=e.ownerDocument||e,e.head.insertBefore(s,i==="title"?e.querySelector("head > title"):null)}function JS(e,i,s){if(s===1||i.itemProp!=null)return!1;switch(e){case"meta":case"title":return!0;case"style":if(typeof i.precedence!="string"||typeof i.href!="string"||i.href==="")break;return!0;case"link":if(typeof i.rel!="string"||typeof i.href!="string"||i.href===""||i.onLoad||i.onError)break;switch(i.rel){case"stylesheet":return e=i.disabled,typeof i.precedence=="string"&&e==null;default:return!0}case"script":if(i.async&&typeof i.async!="function"&&typeof i.async!="symbol"&&!i.onLoad&&!i.onError&&i.src&&typeof i.src=="string")return!0}return!1}function g_(e){return!(e.type==="stylesheet"&&(e.state.loading&3)===0)}function $S(e,i,s,o){if(s.type==="stylesheet"&&(typeof o.media!="string"||matchMedia(o.media).matches!==!1)&&(s.state.loading&4)===0){if(s.instance===null){var u=Dr(o.href),p=i.querySelector(Qo(u));if(p){i=p._p,i!==null&&typeof i=="object"&&typeof i.then=="function"&&(e.count++,e=Cc.bind(e),i.then(e,e)),s.state.loading|=4,s.instance=p,mn(p);return}p=i.ownerDocument||i,o=d_(o),(u=bi.get(u))&&Ld(o,u),p=p.createElement("link"),mn(p);var M=p;M._p=new Promise(function(N,H){M.onload=N,M.onerror=H}),Ln(p,"link",o),s.instance=p}e.stylesheets===null&&(e.stylesheets=new Map),e.stylesheets.set(s,i),(i=s.state.preload)&&(s.state.loading&3)===0&&(e.count++,s=Cc.bind(e),i.addEventListener("load",s),i.addEventListener("error",s))}}var Pd=0;function tM(e,i){return e.stylesheets&&e.count===0&&Dc(e,e.stylesheets),0<e.count||0<e.imgCount?function(s){var o=setTimeout(function(){if(e.stylesheets&&Dc(e,e.stylesheets),e.unsuspend){var p=e.unsuspend;e.unsuspend=null,p()}},6e4+i);0<e.imgBytes&&Pd===0&&(Pd=62500*OS());var u=setTimeout(function(){if(e.waitingForImages=!1,e.count===0&&(e.stylesheets&&Dc(e,e.stylesheets),e.unsuspend)){var p=e.unsuspend;e.unsuspend=null,p()}},(e.imgBytes>Pd?50:800)+i);return e.unsuspend=s,function(){e.unsuspend=null,clearTimeout(o),clearTimeout(u)}}:null}function Cc(){if(this.count--,this.count===0&&(this.imgCount===0||!this.waitingForImages)){if(this.stylesheets)Dc(this,this.stylesheets);else if(this.unsuspend){var e=this.unsuspend;this.unsuspend=null,e()}}}var Nc=null;function Dc(e,i){e.stylesheets=null,e.unsuspend!==null&&(e.count++,Nc=new Map,i.forEach(eM,e),Nc=null,Cc.call(e))}function eM(e,i){if(!(i.state.loading&4)){var s=Nc.get(e);if(s)var o=s.get(null);else{s=new Map,Nc.set(e,s);for(var u=e.querySelectorAll("link[data-precedence],style[data-precedence]"),p=0;p<u.length;p++){var M=u[p];(M.nodeName==="LINK"||M.getAttribute("media")!=="not all")&&(s.set(M.dataset.precedence,M),o=M)}o&&s.set(null,o)}u=i.instance,M=u.getAttribute("data-precedence"),p=s.get(M)||o,p===o&&s.set(null,u),s.set(M,u),this.count++,o=Cc.bind(this),u.addEventListener("load",o),u.addEventListener("error",o),p?p.parentNode.insertBefore(u,p.nextSibling):(e=e.nodeType===9?e.head:e,e.insertBefore(u,e.firstChild)),i.state.loading|=4}}var $o={$$typeof:D,Provider:null,Consumer:null,_currentValue:ct,_currentValue2:ct,_threadCount:0};function nM(e,i,s,o,u,p,M,N,H){this.tag=1,this.containerInfo=e,this.pingCache=this.current=this.pendingChildren=null,this.timeoutHandle=-1,this.callbackNode=this.next=this.pendingContext=this.context=this.cancelPendingCommit=null,this.callbackPriority=0,this.expirationTimes=Jt(-1),this.entangledLanes=this.shellSuspendCounter=this.errorRecoveryDisabledLanes=this.expiredLanes=this.warmLanes=this.pingedLanes=this.suspendedLanes=this.pendingLanes=0,this.entanglements=Jt(0),this.hiddenUpdates=Jt(null),this.identifierPrefix=o,this.onUncaughtError=u,this.onCaughtError=p,this.onRecoverableError=M,this.pooledCache=null,this.pooledCacheLanes=0,this.formState=H,this.incompleteTransitions=new Map}function __(e,i,s,o,u,p,M,N,H,nt,mt,St){return e=new nM(e,i,s,M,H,nt,mt,St,N),i=1,p===!0&&(i|=24),p=si(3,null,null,i),e.current=p,p.stateNode=e,i=pf(),i.refCount++,e.pooledCache=i,i.refCount++,p.memoizedState={element:o,isDehydrated:s,cache:i},vf(p),e}function v_(e){return e?(e=cr,e):cr}function x_(e,i,s,o,u,p){u=v_(u),o.context===null?o.context=u:o.pendingContext=u,o=Wa(i),o.payload={element:s},p=p===void 0?null:p,p!==null&&(o.callback=p),s=qa(e,o,i),s!==null&&($n(s,e,i),Do(s,e,i))}function y_(e,i){if(e=e.memoizedState,e!==null&&e.dehydrated!==null){var s=e.retryLane;e.retryLane=s!==0&&s<i?s:i}}function Id(e,i){y_(e,i),(e=e.alternate)&&y_(e,i)}function S_(e){if(e.tag===13||e.tag===31){var i=As(e,67108864);i!==null&&$n(i,e,67108864),Id(e,67108864)}}function M_(e){if(e.tag===13||e.tag===31){var i=ui();i=ys(i);var s=As(e,i);s!==null&&$n(s,e,i),Id(e,i)}}var Uc=!0;function iM(e,i,s,o){var u=P.T;P.T=null;var p=F.p;try{F.p=2,zd(e,i,s,o)}finally{F.p=p,P.T=u}}function aM(e,i,s,o){var u=P.T;P.T=null;var p=F.p;try{F.p=8,zd(e,i,s,o)}finally{F.p=p,P.T=u}}function zd(e,i,s,o){if(Uc){var u=Bd(o);if(u===null)bd(e,i,o,Lc,s),E_(e,o);else if(rM(u,e,i,s,o))o.stopPropagation();else if(E_(e,o),i&4&&-1<sM.indexOf(e)){for(;u!==null;){var p=za(u);if(p!==null)switch(p.tag){case 3:if(p=p.stateNode,p.current.memoizedState.isDehydrated){var M=Ct(p.pendingLanes);if(M!==0){var N=p;for(N.pendingLanes|=2,N.entangledLanes|=2;M;){var H=1<<31-Ft(M);N.entanglements[1]|=H,M&=~H}Xi(p),(ze&6)===0&&(mc=Dt()+500,qo(0))}}break;case 31:case 13:N=As(p,2),N!==null&&$n(N,p,2),_c(),Id(p,2)}if(p=Bd(o),p===null&&bd(e,i,o,Lc,s),p===u)break;u=p}u!==null&&o.stopPropagation()}else bd(e,i,o,null,s)}}function Bd(e){return e=Fu(e),Fd(e)}var Lc=null;function Fd(e){if(Lc=null,e=Ia(e),e!==null){var i=c(e);if(i===null)e=null;else{var s=i.tag;if(s===13){if(e=f(i),e!==null)return e;e=null}else if(s===31){if(e=d(i),e!==null)return e;e=null}else if(s===3){if(i.stateNode.current.memoizedState.isDehydrated)return i.tag===3?i.stateNode.containerInfo:null;e=null}else i!==e&&(e=null)}}return Lc=e,null}function b_(e){switch(e){case"beforetoggle":case"cancel":case"click":case"close":case"contextmenu":case"copy":case"cut":case"auxclick":case"dblclick":case"dragend":case"dragstart":case"drop":case"focusin":case"focusout":case"input":case"invalid":case"keydown":case"keypress":case"keyup":case"mousedown":case"mouseup":case"paste":case"pause":case"play":case"pointercancel":case"pointerdown":case"pointerup":case"ratechange":case"reset":case"resize":case"seeked":case"submit":case"toggle":case"touchcancel":case"touchend":case"touchstart":case"volumechange":case"change":case"selectionchange":case"textInput":case"compositionstart":case"compositionend":case"compositionupdate":case"beforeblur":case"afterblur":case"beforeinput":case"blur":case"fullscreenchange":case"focus":case"hashchange":case"popstate":case"select":case"selectstart":return 2;case"drag":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"mousemove":case"mouseout":case"mouseover":case"pointermove":case"pointerout":case"pointerover":case"scroll":case"touchmove":case"wheel":case"mouseenter":case"mouseleave":case"pointerenter":case"pointerleave":return 8;case"message":switch(an()){case L:return 2;case T:return 8;case tt:case yt:return 32;case At:return 268435456;default:return 32}default:return 32}}var Hd=!1,as=null,ss=null,rs=null,tl=new Map,el=new Map,os=[],sM="mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");function E_(e,i){switch(e){case"focusin":case"focusout":as=null;break;case"dragenter":case"dragleave":ss=null;break;case"mouseover":case"mouseout":rs=null;break;case"pointerover":case"pointerout":tl.delete(i.pointerId);break;case"gotpointercapture":case"lostpointercapture":el.delete(i.pointerId)}}function nl(e,i,s,o,u,p){return e===null||e.nativeEvent!==p?(e={blockedOn:i,domEventName:s,eventSystemFlags:o,nativeEvent:p,targetContainers:[u]},i!==null&&(i=za(i),i!==null&&S_(i)),e):(e.eventSystemFlags|=o,i=e.targetContainers,u!==null&&i.indexOf(u)===-1&&i.push(u),e)}function rM(e,i,s,o,u){switch(i){case"focusin":return as=nl(as,e,i,s,o,u),!0;case"dragenter":return ss=nl(ss,e,i,s,o,u),!0;case"mouseover":return rs=nl(rs,e,i,s,o,u),!0;case"pointerover":var p=u.pointerId;return tl.set(p,nl(tl.get(p)||null,e,i,s,o,u)),!0;case"gotpointercapture":return p=u.pointerId,el.set(p,nl(el.get(p)||null,e,i,s,o,u)),!0}return!1}function T_(e){var i=Ia(e.target);if(i!==null){var s=c(i);if(s!==null){if(i=s.tag,i===13){if(i=f(s),i!==null){e.blockedOn=i,ho(e.priority,function(){M_(s)});return}}else if(i===31){if(i=d(s),i!==null){e.blockedOn=i,ho(e.priority,function(){M_(s)});return}}else if(i===3&&s.stateNode.current.memoizedState.isDehydrated){e.blockedOn=s.tag===3?s.stateNode.containerInfo:null;return}}}e.blockedOn=null}function Oc(e){if(e.blockedOn!==null)return!1;for(var i=e.targetContainers;0<i.length;){var s=Bd(e.nativeEvent);if(s===null){s=e.nativeEvent;var o=new s.constructor(s.type,s);Bu=o,s.target.dispatchEvent(o),Bu=null}else return i=za(s),i!==null&&S_(i),e.blockedOn=s,!1;i.shift()}return!0}function A_(e,i,s){Oc(e)&&s.delete(i)}function oM(){Hd=!1,as!==null&&Oc(as)&&(as=null),ss!==null&&Oc(ss)&&(ss=null),rs!==null&&Oc(rs)&&(rs=null),tl.forEach(A_),el.forEach(A_)}function Pc(e,i){e.blockedOn===i&&(e.blockedOn=null,Hd||(Hd=!0,r.unstable_scheduleCallback(r.unstable_NormalPriority,oM)))}var Ic=null;function w_(e){Ic!==e&&(Ic=e,r.unstable_scheduleCallback(r.unstable_NormalPriority,function(){Ic===e&&(Ic=null);for(var i=0;i<e.length;i+=3){var s=e[i],o=e[i+1],u=e[i+2];if(typeof o!="function"){if(Fd(o||s)===null)continue;break}var p=za(s);p!==null&&(e.splice(i,3),i-=3,Bf(p,{pending:!0,data:u,method:s.method,action:o},o,u))}}))}function Lr(e){function i(H){return Pc(H,e)}as!==null&&Pc(as,e),ss!==null&&Pc(ss,e),rs!==null&&Pc(rs,e),tl.forEach(i),el.forEach(i);for(var s=0;s<os.length;s++){var o=os[s];o.blockedOn===e&&(o.blockedOn=null)}for(;0<os.length&&(s=os[0],s.blockedOn===null);)T_(s),s.blockedOn===null&&os.shift();if(s=(e.ownerDocument||e).$$reactFormReplay,s!=null)for(o=0;o<s.length;o+=3){var u=s[o],p=s[o+1],M=u[Cn]||null;if(typeof p=="function")M||w_(s);else if(M){var N=null;if(p&&p.hasAttribute("formAction")){if(u=p,M=p[Cn]||null)N=M.formAction;else if(Fd(u)!==null)continue}else N=M.action;typeof N=="function"?s[o+1]=N:(s.splice(o,3),o-=3),w_(s)}}}function R_(){function e(p){p.canIntercept&&p.info==="react-transition"&&p.intercept({handler:function(){return new Promise(function(M){return u=M})},focusReset:"manual",scroll:"manual"})}function i(){u!==null&&(u(),u=null),o||setTimeout(s,20)}function s(){if(!o&&!navigation.transition){var p=navigation.currentEntry;p&&p.url!=null&&navigation.navigate(p.url,{state:p.getState(),info:"react-transition",history:"replace"})}}if(typeof navigation=="object"){var o=!1,u=null;return navigation.addEventListener("navigate",e),navigation.addEventListener("navigatesuccess",i),navigation.addEventListener("navigateerror",i),setTimeout(s,100),function(){o=!0,navigation.removeEventListener("navigate",e),navigation.removeEventListener("navigatesuccess",i),navigation.removeEventListener("navigateerror",i),u!==null&&(u(),u=null)}}}function Gd(e){this._internalRoot=e}zc.prototype.render=Gd.prototype.render=function(e){var i=this._internalRoot;if(i===null)throw Error(a(409));var s=i.current,o=ui();x_(s,o,e,i,null,null)},zc.prototype.unmount=Gd.prototype.unmount=function(){var e=this._internalRoot;if(e!==null){this._internalRoot=null;var i=e.containerInfo;x_(e.current,2,null,e,null,null),_c(),i[ia]=null}};function zc(e){this._internalRoot=e}zc.prototype.unstable_scheduleHydration=function(e){if(e){var i=fo();e={blockedOn:null,target:e,priority:i};for(var s=0;s<os.length&&i!==0&&i<os[s].priority;s++);os.splice(s,0,e),s===0&&T_(e)}};var C_=t.version;if(C_!=="19.2.5")throw Error(a(527,C_,"19.2.5"));F.findDOMNode=function(e){var i=e._reactInternals;if(i===void 0)throw typeof e.render=="function"?Error(a(188)):(e=Object.keys(e).join(","),Error(a(268,e)));return e=h(i),e=e!==null?g(e):null,e=e===null?null:e.stateNode,e};var lM={bundleType:0,version:"19.2.5",rendererPackageName:"react-dom",currentDispatcherRef:P,reconcilerVersion:"19.2.5"};if(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__<"u"){var Bc=__REACT_DEVTOOLS_GLOBAL_HOOK__;if(!Bc.isDisabled&&Bc.supportsFiber)try{dt=Bc.inject(lM),pt=Bc}catch{}}return al.createRoot=function(e,i){if(!l(e))throw Error(a(299));var s=!1,o="",u=Ig,p=zg,M=Bg;return i!=null&&(i.unstable_strictMode===!0&&(s=!0),i.identifierPrefix!==void 0&&(o=i.identifierPrefix),i.onUncaughtError!==void 0&&(u=i.onUncaughtError),i.onCaughtError!==void 0&&(p=i.onCaughtError),i.onRecoverableError!==void 0&&(M=i.onRecoverableError)),i=__(e,1,!1,null,null,s,o,null,u,p,M,R_),e[ia]=i.current,Md(e),new Gd(i)},al.hydrateRoot=function(e,i,s){if(!l(e))throw Error(a(299));var o=!1,u="",p=Ig,M=zg,N=Bg,H=null;return s!=null&&(s.unstable_strictMode===!0&&(o=!0),s.identifierPrefix!==void 0&&(u=s.identifierPrefix),s.onUncaughtError!==void 0&&(p=s.onUncaughtError),s.onCaughtError!==void 0&&(M=s.onCaughtError),s.onRecoverableError!==void 0&&(N=s.onRecoverableError),s.formState!==void 0&&(H=s.formState)),i=__(e,1,!0,i,s??null,o,u,H,p,M,N,R_),i.context=v_(null),s=i.current,o=ui(),o=ys(o),u=Wa(o),u.callback=null,qa(s,u,o),s=o,i.current.lanes=s,ue(i,s),Xi(i),e[ia]=i.current,Md(e),new zc(i)},al.version="19.2.5",al}var F_;function vM(){if(F_)return jd.exports;F_=1;function r(){if(!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__>"u"||typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE!="function"))try{__REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r)}catch(t){console.error(t)}}return r(),jd.exports=_M(),jd.exports}var xM=vM();function yM(r,t,n,a){if(typeof t=="function"?r!==t||!a:!t.has(r))throw new TypeError("Cannot read private member from an object whose class did not declare it");return n==="m"?a:n==="a"?a.call(r):a?a.value:t.get(r)}function SM(r,t,n,a,l){if(typeof t=="function"?r!==t||!0:!t.has(r))throw new TypeError("Cannot write private member to an object whose class did not declare it");return t.set(r,n),n}var hu;const Ni="__TAURI_TO_IPC_KEY__";function MM(r,t=!1){return window.__TAURI_INTERNALS__.transformCallback(r,t)}async function rt(r,t={},n){return window.__TAURI_INTERNALS__.invoke(r,t,n)}class bM{get rid(){return yM(this,hu,"f")}constructor(t){hu.set(this,void 0),SM(this,hu,t)}async close(){return rt("plugin:resources|close",{rid:this.rid})}}hu=new WeakMap;class ux{constructor(...t){this.type="Logical",t.length===1?"Logical"in t[0]?(this.width=t[0].Logical.width,this.height=t[0].Logical.height):(this.width=t[0].width,this.height=t[0].height):(this.width=t[0],this.height=t[1])}toPhysical(t){return new no(this.width*t,this.height*t)}[Ni](){return{width:this.width,height:this.height}}toJSON(){return this[Ni]()}}class no{constructor(...t){this.type="Physical",t.length===1?"Physical"in t[0]?(this.width=t[0].Physical.width,this.height=t[0].Physical.height):(this.width=t[0].width,this.height=t[0].height):(this.width=t[0],this.height=t[1])}toLogical(t){return new ux(this.width/t,this.height/t)}[Ni](){return{width:this.width,height:this.height}}toJSON(){return this[Ni]()}}class ms{constructor(t){this.size=t}toLogical(t){return this.size instanceof ux?this.size:this.size.toLogical(t)}toPhysical(t){return this.size instanceof no?this.size:this.size.toPhysical(t)}[Ni](){return{[`${this.size.type}`]:{width:this.size.width,height:this.size.height}}}toJSON(){return this[Ni]()}}class fx{constructor(...t){this.type="Logical",t.length===1?"Logical"in t[0]?(this.x=t[0].Logical.x,this.y=t[0].Logical.y):(this.x=t[0].x,this.y=t[0].y):(this.x=t[0],this.y=t[1])}toPhysical(t){return new Ai(this.x*t,this.y*t)}[Ni](){return{x:this.x,y:this.y}}toJSON(){return this[Ni]()}}class Ai{constructor(...t){this.type="Physical",t.length===1?"Physical"in t[0]?(this.x=t[0].Physical.x,this.y=t[0].Physical.y):(this.x=t[0].x,this.y=t[0].y):(this.x=t[0],this.y=t[1])}toLogical(t){return new fx(this.x/t,this.y/t)}[Ni](){return{x:this.x,y:this.y}}toJSON(){return this[Ni]()}}class to{constructor(t){this.position=t}toLogical(t){return this.position instanceof fx?this.position:this.position.toLogical(t)}toPhysical(t){return this.position instanceof Ai?this.position:this.position.toPhysical(t)}[Ni](){return{[`${this.position.type}`]:{x:this.position.x,y:this.position.y}}}toJSON(){return this[Ni]()}}var On;(function(r){r.WINDOW_RESIZED="tauri://resize",r.WINDOW_MOVED="tauri://move",r.WINDOW_CLOSE_REQUESTED="tauri://close-requested",r.WINDOW_DESTROYED="tauri://destroyed",r.WINDOW_FOCUS="tauri://focus",r.WINDOW_BLUR="tauri://blur",r.WINDOW_SCALE_FACTOR_CHANGED="tauri://scale-change",r.WINDOW_THEME_CHANGED="tauri://theme-changed",r.WINDOW_CREATED="tauri://window-created",r.WINDOW_SUSPENDED="tauri://suspended",r.WINDOW_RESUMED="tauri://resumed",r.WEBVIEW_CREATED="tauri://webview-created",r.DRAG_ENTER="tauri://drag-enter",r.DRAG_OVER="tauri://drag-over",r.DRAG_DROP="tauri://drag-drop",r.DRAG_LEAVE="tauri://drag-leave"})(On||(On={}));async function dx(r,t){window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(r,t),await rt("plugin:event|unlisten",{event:r,eventId:t})}async function $e(r,t,n){var a;const l=typeof n?.target=="string"?{kind:"AnyLabel",label:n.target}:(a=n?.target)!==null&&a!==void 0?a:{kind:"Any"};return rt("plugin:event|listen",{event:r,target:l,handler:MM(t)}).then(c=>async()=>dx(r,c))}async function Lp(r,t,n){return $e(r,a=>{dx(r,a.id),t(a)},n)}async function hx(r,t){await rt("plugin:event|emit",{event:r,payload:t})}async function px(r,t,n){await rt("plugin:event|emit_to",{target:typeof r=="string"?{kind:"AnyLabel",label:r}:r,event:t,payload:n})}class gl extends bM{constructor(t){super(t)}static async new(t,n,a){return rt("plugin:image|new",{rgba:Su(t),width:n,height:a}).then(l=>new gl(l))}static async fromBytes(t){return rt("plugin:image|from_bytes",{bytes:Su(t)}).then(n=>new gl(n))}static async fromPath(t){return rt("plugin:image|from_path",{path:t}).then(n=>new gl(n))}async rgba(){return rt("plugin:image|rgba",{rid:this.rid}).then(t=>new Uint8Array(t))}async size(){return rt("plugin:image|size",{rid:this.rid})}}function Su(r){return r==null?null:typeof r=="string"?r:r instanceof gl?r.rid:r}var Ph;(function(r){r[r.Critical=1]="Critical",r[r.Informational=2]="Informational"})(Ph||(Ph={}));class EM{constructor(t){this._preventDefault=!1,this.event=t.event,this.id=t.id}preventDefault(){this._preventDefault=!0}isPreventDefault(){return this._preventDefault}}var H_;(function(r){r.None="none",r.Normal="normal",r.Indeterminate="indeterminate",r.Paused="paused",r.Error="error"})(H_||(H_={}));function mx(){return new Uu(window.__TAURI_INTERNALS__.metadata.currentWindow.label,{skip:!0})}async function Yd(){return rt("plugin:window|get_all_windows").then(r=>r.map(t=>new Uu(t,{skip:!0})))}const Zd=["tauri://created","tauri://error"];class Uu{constructor(t,n={}){var a;this.label=t,this.listeners=Object.create(null),n?.skip||rt("plugin:window|create",{options:{...n,parent:typeof n.parent=="string"?n.parent:(a=n.parent)===null||a===void 0?void 0:a.label,label:t}}).then(async()=>this.emit("tauri://created")).catch(async l=>this.emit("tauri://error",l))}static async getByLabel(t){var n;return(n=(await Yd()).find(a=>a.label===t))!==null&&n!==void 0?n:null}static getCurrent(){return mx()}static async getAll(){return Yd()}static async getFocusedWindow(){for(const t of await Yd())if(await t.isFocused())return t;return null}async listen(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:$e(t,n,{target:{kind:"Window",label:this.label}})}async once(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:Lp(t,n,{target:{kind:"Window",label:this.label}})}async emit(t,n){if(Zd.includes(t)){for(const a of this.listeners[t]||[])a({event:t,id:-1,payload:n});return}return hx(t,n)}async emitTo(t,n,a){if(Zd.includes(n)){for(const l of this.listeners[n]||[])l({event:n,id:-1,payload:a});return}return px(t,n,a)}_handleTauriEvent(t,n){return Zd.includes(t)?(t in this.listeners?this.listeners[t].push(n):this.listeners[t]=[n],!0):!1}async scaleFactor(){return rt("plugin:window|scale_factor",{label:this.label})}async innerPosition(){return rt("plugin:window|inner_position",{label:this.label}).then(t=>new Ai(t))}async outerPosition(){return rt("plugin:window|outer_position",{label:this.label}).then(t=>new Ai(t))}async innerSize(){return rt("plugin:window|inner_size",{label:this.label}).then(t=>new no(t))}async outerSize(){return rt("plugin:window|outer_size",{label:this.label}).then(t=>new no(t))}async isFullscreen(){return rt("plugin:window|is_fullscreen",{label:this.label})}async isMinimized(){return rt("plugin:window|is_minimized",{label:this.label})}async isMaximized(){return rt("plugin:window|is_maximized",{label:this.label})}async isFocused(){return rt("plugin:window|is_focused",{label:this.label})}async isDecorated(){return rt("plugin:window|is_decorated",{label:this.label})}async isResizable(){return rt("plugin:window|is_resizable",{label:this.label})}async isMaximizable(){return rt("plugin:window|is_maximizable",{label:this.label})}async isMinimizable(){return rt("plugin:window|is_minimizable",{label:this.label})}async isClosable(){return rt("plugin:window|is_closable",{label:this.label})}async isVisible(){return rt("plugin:window|is_visible",{label:this.label})}async title(){return rt("plugin:window|title",{label:this.label})}async theme(){return rt("plugin:window|theme",{label:this.label})}async isAlwaysOnTop(){return rt("plugin:window|is_always_on_top",{label:this.label})}async activityName(){return rt("plugin:window|activity_name",{label:this.label})}async sceneIdentifier(){return rt("plugin:window|scene_identifier",{label:this.label})}async center(){return rt("plugin:window|center",{label:this.label})}async requestUserAttention(t){let n=null;return t&&(t===Ph.Critical?n={type:"Critical"}:n={type:"Informational"}),rt("plugin:window|request_user_attention",{label:this.label,value:n})}async setResizable(t){return rt("plugin:window|set_resizable",{label:this.label,value:t})}async setEnabled(t){return rt("plugin:window|set_enabled",{label:this.label,value:t})}async isEnabled(){return rt("plugin:window|is_enabled",{label:this.label})}async setMaximizable(t){return rt("plugin:window|set_maximizable",{label:this.label,value:t})}async setMinimizable(t){return rt("plugin:window|set_minimizable",{label:this.label,value:t})}async setClosable(t){return rt("plugin:window|set_closable",{label:this.label,value:t})}async setTitle(t){return rt("plugin:window|set_title",{label:this.label,value:t})}async maximize(){return rt("plugin:window|maximize",{label:this.label})}async unmaximize(){return rt("plugin:window|unmaximize",{label:this.label})}async toggleMaximize(){return rt("plugin:window|toggle_maximize",{label:this.label})}async minimize(){return rt("plugin:window|minimize",{label:this.label})}async unminimize(){return rt("plugin:window|unminimize",{label:this.label})}async show(){return rt("plugin:window|show",{label:this.label})}async hide(){return rt("plugin:window|hide",{label:this.label})}async close(){return rt("plugin:window|close",{label:this.label})}async destroy(){return rt("plugin:window|destroy",{label:this.label})}async setDecorations(t){return rt("plugin:window|set_decorations",{label:this.label,value:t})}async setShadow(t){return rt("plugin:window|set_shadow",{label:this.label,value:t})}async setEffects(t){return rt("plugin:window|set_effects",{label:this.label,value:t})}async clearEffects(){return rt("plugin:window|set_effects",{label:this.label,value:null})}async setAlwaysOnTop(t){return rt("plugin:window|set_always_on_top",{label:this.label,value:t})}async setAlwaysOnBottom(t){return rt("plugin:window|set_always_on_bottom",{label:this.label,value:t})}async setContentProtected(t){return rt("plugin:window|set_content_protected",{label:this.label,value:t})}async setSize(t){return rt("plugin:window|set_size",{label:this.label,value:t instanceof ms?t:new ms(t)})}async setMinSize(t){return rt("plugin:window|set_min_size",{label:this.label,value:t instanceof ms?t:t?new ms(t):null})}async setMaxSize(t){return rt("plugin:window|set_max_size",{label:this.label,value:t instanceof ms?t:t?new ms(t):null})}async setSizeConstraints(t){function n(a){return a?{Logical:a}:null}return rt("plugin:window|set_size_constraints",{label:this.label,value:{minWidth:n(t?.minWidth),minHeight:n(t?.minHeight),maxWidth:n(t?.maxWidth),maxHeight:n(t?.maxHeight)}})}async setPosition(t){return rt("plugin:window|set_position",{label:this.label,value:t instanceof to?t:new to(t)})}async setFullscreen(t){return rt("plugin:window|set_fullscreen",{label:this.label,value:t})}async setSimpleFullscreen(t){return rt("plugin:window|set_simple_fullscreen",{label:this.label,value:t})}async setFocus(){return rt("plugin:window|set_focus",{label:this.label})}async setFocusable(t){return rt("plugin:window|set_focusable",{label:this.label,value:t})}async setIcon(t){return rt("plugin:window|set_icon",{label:this.label,value:Su(t)})}async setSkipTaskbar(t){return rt("plugin:window|set_skip_taskbar",{label:this.label,value:t})}async setCursorGrab(t){return rt("plugin:window|set_cursor_grab",{label:this.label,value:t})}async setCursorVisible(t){return rt("plugin:window|set_cursor_visible",{label:this.label,value:t})}async setCursorIcon(t){return rt("plugin:window|set_cursor_icon",{label:this.label,value:t})}async setBackgroundColor(t){return rt("plugin:window|set_background_color",{color:t})}async setCursorPosition(t){return rt("plugin:window|set_cursor_position",{label:this.label,value:t instanceof to?t:new to(t)})}async setIgnoreCursorEvents(t){return rt("plugin:window|set_ignore_cursor_events",{label:this.label,value:t})}async startDragging(){return rt("plugin:window|start_dragging",{label:this.label})}async startResizeDragging(t){return rt("plugin:window|start_resize_dragging",{label:this.label,value:t})}async setBadgeCount(t){return rt("plugin:window|set_badge_count",{label:this.label,value:t})}async setBadgeLabel(t){return rt("plugin:window|set_badge_label",{label:this.label,value:t})}async setOverlayIcon(t){return rt("plugin:window|set_overlay_icon",{label:this.label,value:t?Su(t):void 0})}async setProgressBar(t){return rt("plugin:window|set_progress_bar",{label:this.label,value:t})}async setVisibleOnAllWorkspaces(t){return rt("plugin:window|set_visible_on_all_workspaces",{label:this.label,value:t})}async setTitleBarStyle(t){return rt("plugin:window|set_title_bar_style",{label:this.label,value:t})}async setTheme(t){return rt("plugin:window|set_theme",{label:this.label,value:t})}async onResized(t){return this.listen(On.WINDOW_RESIZED,n=>{n.payload=new no(n.payload),t(n)})}async onMoved(t){return this.listen(On.WINDOW_MOVED,n=>{n.payload=new Ai(n.payload),t(n)})}async onCloseRequested(t){return this.listen(On.WINDOW_CLOSE_REQUESTED,async n=>{const a=new EM(n);await t(a),a.isPreventDefault()||await this.destroy()})}async onDragDropEvent(t){const n=await this.listen(On.DRAG_ENTER,f=>{t({...f,payload:{type:"enter",paths:f.payload.paths,position:new Ai(f.payload.position)}})}),a=await this.listen(On.DRAG_OVER,f=>{t({...f,payload:{type:"over",position:new Ai(f.payload.position)}})}),l=await this.listen(On.DRAG_DROP,f=>{t({...f,payload:{type:"drop",paths:f.payload.paths,position:new Ai(f.payload.position)}})}),c=await this.listen(On.DRAG_LEAVE,f=>{t({...f,payload:{type:"leave"}})});return()=>{n(),l(),a(),c()}}async onFocusChanged(t){const n=await this.listen(On.WINDOW_FOCUS,l=>{t({...l,payload:!0})}),a=await this.listen(On.WINDOW_BLUR,l=>{t({...l,payload:!1})});return()=>{n(),a()}}async onScaleChanged(t){return this.listen(On.WINDOW_SCALE_FACTOR_CHANGED,t)}async onThemeChanged(t){return this.listen(On.WINDOW_THEME_CHANGED,t)}}var G_;(function(r){r.Disabled="disabled",r.Throttle="throttle",r.Suspend="suspend"})(G_||(G_={}));var V_;(function(r){r.Default="default",r.FluentOverlay="fluentOverlay"})(V_||(V_={}));var k_;(function(r){r.AppearanceBased="appearanceBased",r.Light="light",r.Dark="dark",r.MediumLight="mediumLight",r.UltraDark="ultraDark",r.Titlebar="titlebar",r.Selection="selection",r.Menu="menu",r.Popover="popover",r.Sidebar="sidebar",r.HeaderView="headerView",r.Sheet="sheet",r.WindowBackground="windowBackground",r.HudWindow="hudWindow",r.FullScreenUI="fullScreenUI",r.Tooltip="tooltip",r.ContentBackground="contentBackground",r.UnderWindowBackground="underWindowBackground",r.UnderPageBackground="underPageBackground",r.Mica="mica",r.Blur="blur",r.Acrylic="acrylic",r.Tabbed="tabbed",r.TabbedDark="tabbedDark",r.TabbedLight="tabbedLight"})(k_||(k_={}));var j_;(function(r){r.FollowsWindowActiveState="followsWindowActiveState",r.Active="active",r.Inactive="inactive"})(j_||(j_={}));function gx(){return new Op(mx(),window.__TAURI_INTERNALS__.metadata.currentWebview.label,{skip:!0})}async function X_(){return rt("plugin:webview|get_all_webviews").then(r=>r.map(t=>new Op(new Uu(t.windowLabel,{skip:!0}),t.label,{skip:!0})))}const Kd=["tauri://created","tauri://error"];class Op{constructor(t,n,a){this.window=t,this.label=n,this.listeners=Object.create(null),a?.skip||rt("plugin:webview|create_webview",{windowLabel:t.label,options:{...a,label:n}}).then(async()=>this.emit("tauri://created")).catch(async l=>this.emit("tauri://error",l))}static async getByLabel(t){var n;return(n=(await X_()).find(a=>a.label===t))!==null&&n!==void 0?n:null}static getCurrent(){return gx()}static async getAll(){return X_()}async listen(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:$e(t,n,{target:{kind:"Webview",label:this.label}})}async once(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:Lp(t,n,{target:{kind:"Webview",label:this.label}})}async emit(t,n){if(Kd.includes(t)){for(const a of this.listeners[t]||[])a({event:t,id:-1,payload:n});return}return hx(t,n)}async emitTo(t,n,a){if(Kd.includes(n)){for(const l of this.listeners[n]||[])l({event:n,id:-1,payload:a});return}return px(t,n,a)}_handleTauriEvent(t,n){return Kd.includes(t)?(t in this.listeners?this.listeners[t].push(n):this.listeners[t]=[n],!0):!1}async position(){return rt("plugin:webview|webview_position",{label:this.label}).then(t=>new Ai(t))}async size(){return rt("plugin:webview|webview_size",{label:this.label}).then(t=>new no(t))}async close(){return rt("plugin:webview|webview_close",{label:this.label})}async setSize(t){return rt("plugin:webview|set_webview_size",{label:this.label,value:t instanceof ms?t:new ms(t)})}async setPosition(t){return rt("plugin:webview|set_webview_position",{label:this.label,value:t instanceof to?t:new to(t)})}async setFocus(){return rt("plugin:webview|set_webview_focus",{label:this.label})}async setAutoResize(t){return rt("plugin:webview|set_webview_auto_resize",{label:this.label,value:t})}async hide(){return rt("plugin:webview|webview_hide",{label:this.label})}async show(){return rt("plugin:webview|webview_show",{label:this.label})}async setZoom(t){return rt("plugin:webview|set_webview_zoom",{label:this.label,value:t})}async reparent(t){return rt("plugin:webview|reparent",{label:this.label,window:typeof t=="string"?t:t.label})}async clearAllBrowsingData(){return rt("plugin:webview|clear_all_browsing_data")}async setBackgroundColor(t){return rt("plugin:webview|set_webview_background_color",{color:t})}async onDragDropEvent(t){const n=await this.listen(On.DRAG_ENTER,f=>{t({...f,payload:{type:"enter",paths:f.payload.paths,position:new Ai(f.payload.position)}})}),a=await this.listen(On.DRAG_OVER,f=>{t({...f,payload:{type:"over",position:new Ai(f.payload.position)}})}),l=await this.listen(On.DRAG_DROP,f=>{t({...f,payload:{type:"drop",paths:f.payload.paths,position:new Ai(f.payload.position)}})}),c=await this.listen(On.DRAG_LEAVE,f=>{t({...f,payload:{type:"leave"}})});return()=>{n(),l(),a(),c()}}}function Pp(){const r=gx();return new Al(r.label,{skip:!0})}async function W_(){return rt("plugin:window|get_all_windows").then(r=>r.map(t=>new Al(t,{skip:!0})))}class Al{constructor(t,n={}){var a;this.label=t,this.listeners=Object.create(null),n?.skip||rt("plugin:webview|create_webview_window",{options:{...n,parent:typeof n.parent=="string"?n.parent:(a=n.parent)===null||a===void 0?void 0:a.label,label:t}}).then(async()=>this.emit("tauri://created")).catch(async l=>this.emit("tauri://error",l))}static async getByLabel(t){var n;const a=(n=(await W_()).find(l=>l.label===t))!==null&&n!==void 0?n:null;return a?new Al(a.label,{skip:!0}):null}static getCurrent(){return Pp()}static async getAll(){return W_()}async listen(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:$e(t,n,{target:{kind:"WebviewWindow",label:this.label}})}async once(t,n){return this._handleTauriEvent(t,n)?()=>{const a=this.listeners[t];a.splice(a.indexOf(n),1)}:Lp(t,n,{target:{kind:"WebviewWindow",label:this.label}})}async setBackgroundColor(t){return rt("plugin:window|set_background_color",{color:t}).then(()=>rt("plugin:webview|set_webview_background_color",{color:t}))}}TM(Al,[Uu,Op]);function TM(r,t){(Array.isArray(t)?t:[t]).forEach(n=>{Object.getOwnPropertyNames(n.prototype).forEach(a=>{var l;typeof r.prototype=="object"&&r.prototype&&a in r.prototype||Object.defineProperty(r.prototype,a,(l=Object.getOwnPropertyDescriptor(n.prototype,a))!==null&&l!==void 0?l:Object.create(null))})})}const AM={all:"All",stone_joined:"Stone joined",stone_left:"Stone offline",storage_activity:"Storage activity"};function wM({onClose:r}){const[t,n]=ut.useState([]),[a,l]=ut.useState("all"),[c,f]=ut.useState("all"),d=ut.useCallback(async()=>{try{const g=await rt("get_activity");n(g)}catch(g){console.error("get_activity failed:",g)}},[]);ut.useEffect(()=>{let g,_=!1;return(async()=>(await d(),g=await $e("activity-changed",()=>{_||d()})))(),()=>{_=!0,g?.()}},[d]);const m=ut.useMemo(()=>t.filter(g=>!(a!=="all"&&g.event.kind!==a||c==="promoted"&&!g.promoted||c==="quiet"&&g.promoted)),[t,a,c]),h=ut.useMemo(()=>CM(m),[m]);return b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"}),b.jsxs("div",{className:"topbar-clock",children:[t.length," entr",t.length===1?"y":"ies"]})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Activity"}),b.jsx("p",{className:"subtle",children:"Every accepted event lands here, whether it fired a toast or stayed quiet. The ring buffer holds the most recent 200."})]}),b.jsxs("section",{className:"activity-filters",children:[b.jsx(q_,{label:"Kind",options:["all","stone_joined","stone_left","storage_activity"].map(g=>({value:g,label:AM[g]})),value:a,onChange:g=>l(g)}),b.jsx(q_,{label:"Toast",options:[{value:"all",label:"All"},{value:"promoted",label:"Promoted"},{value:"quiet",label:"Quiet"}],value:c,onChange:g=>f(g)})]}),m.length===0?b.jsx("section",{className:"settings-empty",children:t.length===0?"Nothing has happened yet. Discover or tend a stone to start filling the feed.":"No entries match these filters."}):b.jsx("section",{className:"activity-feed",children:h.map(g=>b.jsxs("div",{className:"activity-group",children:[b.jsx("div",{className:"activity-group-heading",children:g.label}),g.entries.map(_=>b.jsx(RM,{entry:_},_.id))]},g.label))})]})}function RM({entry:r}){const{primary:t,secondary:n}=DM(r.event),a=NM(r.at);return b.jsxs("article",{className:"activity-entry",children:[b.jsx("span",{className:`severity-pip severity-${r.severity}`}),b.jsxs("div",{className:"activity-entry-body",children:[b.jsxs("div",{className:"activity-entry-primary",children:[t,r.promoted&&b.jsx("span",{className:"activity-entry-promoted",title:"A toast fired for this event",children:"toasted"})]}),b.jsx("div",{className:"activity-entry-secondary",children:n})]}),b.jsx("span",{className:"activity-entry-time",children:a})]})}function q_({label:r,options:t,value:n,onChange:a}){return b.jsxs("div",{className:"activity-filter",children:[b.jsx("span",{className:"activity-filter-label",children:r}),b.jsx("div",{className:"activity-filter-chips",children:t.map(l=>b.jsx("button",{type:"button",className:`activity-filter-chip ${n===l.value?"activity-filter-chip-on":""}`,onClick:()=>a(l.value),children:l.label},l.value))})]})}function CM(r){if(r.length===0)return[];const t=Y_(new Date),n=new Date(t.getTime()-864e5),a=[];let l=null;for(const c of r){const f=new Date(c.at),d=Y_(f),m=d.getTime()===t.getTime()?"Today":d.getTime()===n.getTime()?"Yesterday":d.toLocaleDateString(void 0,{weekday:"long",month:"short",day:"numeric"});(!l||l.label!==m)&&(l={label:m,entries:[]},a.push(l)),l.entries.push(c)}return a}function Y_(r){return new Date(r.getFullYear(),r.getMonth(),r.getDate())}function NM(r){return new Date(r).toLocaleTimeString(void 0,{hour:"2-digit",minute:"2-digit"})}function DM(r){switch(r.kind){case"stone_joined":return{primary:`${r.stone_name} joined`,secondary:r.endpoint};case"stone_left":return{primary:`${r.stone_name} offline`,secondary:"lost contact"};case"storage_activity":{const t=r.creates+r.modifies+r.deletes;return{primary:`${r.bank_name} synced ${t} files on ${r.stone_name}`,secondary:`${r.creates} new · ${r.modifies} changed · ${r.deletes} removed`}}}}/**
 * @license
 * Copyright 2010-2026 Three.js Authors
 * SPDX-License-Identifier: MIT
 */const Ip="184",UM=0,Z_=1,LM=2,pu=1,OM=2,pl=3,vs=0,ti=1,Aa=2,Ra=0,io=1,_l=2,K_=3,Q_=4,PM=5,Xs=100,IM=101,zM=102,BM=103,FM=104,HM=200,GM=201,VM=202,kM=203,Ih=204,zh=205,jM=206,XM=207,WM=208,qM=209,YM=210,ZM=211,KM=212,QM=213,JM=214,Bh=0,Fh=1,Hh=2,so=3,Gh=4,Vh=5,kh=6,jh=7,_x=0,$M=1,tb=2,$i=0,vx=1,xx=2,yx=3,Sx=4,Mx=5,bx=6,Ex=7,Tx=300,Js=301,ro=302,Qd=303,Jd=304,Lu=306,Xh=1e3,wa=1001,Wh=1002,Pn=1003,eb=1004,Fc=1005,Sn=1006,$d=1007,Zs=1008,pi=1009,Ax=1010,wx=1011,Ml=1012,zp=1013,ea=1014,Qi=1015,Da=1016,Bp=1017,Fp=1018,bl=1020,Rx=35902,Cx=35899,Nx=1021,Dx=1022,Hi=1023,Ua=1026,Ks=1027,Ux=1028,Hp=1029,$s=1030,Gp=1031,Vp=1033,mu=33776,gu=33777,_u=33778,vu=33779,qh=35840,Yh=35841,Zh=35842,Kh=35843,Qh=36196,Jh=37492,$h=37496,tp=37488,ep=37489,Mu=37490,np=37491,ip=37808,ap=37809,sp=37810,rp=37811,op=37812,lp=37813,cp=37814,up=37815,fp=37816,dp=37817,hp=37818,pp=37819,mp=37820,gp=37821,_p=36492,vp=36494,xp=36495,yp=36283,Sp=36284,bu=36285,Mp=36286,nb=3200,bp=0,ib=1,gs="",Ti="srgb",Eu="srgb-linear",Tu="linear",je="srgb",Or=7680,J_=519,ab=512,sb=513,rb=514,kp=515,ob=516,lb=517,jp=518,cb=519,Ep=35044,$_="300 es",Ji=2e3,El=2001;function ub(r){for(let t=r.length-1;t>=0;--t)if(r[t]>=65535)return!0;return!1}function Au(r){return document.createElementNS("http://www.w3.org/1999/xhtml",r)}function fb(){const r=Au("canvas");return r.style.display="block",r}const tv={};function wu(...r){const t="THREE."+r.shift();console.log(t,...r)}function Lx(r){const t=r[0];if(typeof t=="string"&&t.startsWith("TSL:")){const n=r[1];n&&n.isStackTrace?r[0]+=" "+n.getLocation():r[1]='Stack trace not available. Enable "THREE.Node.captureStackTrace" to capture stack traces.'}return r}function ce(...r){r=Lx(r);const t="THREE."+r.shift();{const n=r[0];n&&n.isStackTrace?console.warn(n.getError(t)):console.warn(t,...r)}}function Ne(...r){r=Lx(r);const t="THREE."+r.shift();{const n=r[0];n&&n.isStackTrace?console.error(n.getError(t)):console.error(t,...r)}}function Tp(...r){const t=r.join(" ");t in tv||(tv[t]=!0,ce(...r))}function db(r,t,n){return new Promise(function(a,l){function c(){switch(r.clientWaitSync(t,r.SYNC_FLUSH_COMMANDS_BIT,0)){case r.WAIT_FAILED:l();break;case r.TIMEOUT_EXPIRED:setTimeout(c,n);break;default:a()}}setTimeout(c,n)})}const hb={[Bh]:Fh,[Hh]:kh,[Gh]:jh,[so]:Vh,[Fh]:Bh,[kh]:Hh,[jh]:Gh,[Vh]:so};class tr{addEventListener(t,n){this._listeners===void 0&&(this._listeners={});const a=this._listeners;a[t]===void 0&&(a[t]=[]),a[t].indexOf(n)===-1&&a[t].push(n)}hasEventListener(t,n){const a=this._listeners;return a===void 0?!1:a[t]!==void 0&&a[t].indexOf(n)!==-1}removeEventListener(t,n){const a=this._listeners;if(a===void 0)return;const l=a[t];if(l!==void 0){const c=l.indexOf(n);c!==-1&&l.splice(c,1)}}dispatchEvent(t){const n=this._listeners;if(n===void 0)return;const a=n[t.type];if(a!==void 0){t.target=this;const l=a.slice(0);for(let c=0,f=l.length;c<f;c++)l[c].call(this,t);t.target=null}}}const Fn=["00","01","02","03","04","05","06","07","08","09","0a","0b","0c","0d","0e","0f","10","11","12","13","14","15","16","17","18","19","1a","1b","1c","1d","1e","1f","20","21","22","23","24","25","26","27","28","29","2a","2b","2c","2d","2e","2f","30","31","32","33","34","35","36","37","38","39","3a","3b","3c","3d","3e","3f","40","41","42","43","44","45","46","47","48","49","4a","4b","4c","4d","4e","4f","50","51","52","53","54","55","56","57","58","59","5a","5b","5c","5d","5e","5f","60","61","62","63","64","65","66","67","68","69","6a","6b","6c","6d","6e","6f","70","71","72","73","74","75","76","77","78","79","7a","7b","7c","7d","7e","7f","80","81","82","83","84","85","86","87","88","89","8a","8b","8c","8d","8e","8f","90","91","92","93","94","95","96","97","98","99","9a","9b","9c","9d","9e","9f","a0","a1","a2","a3","a4","a5","a6","a7","a8","a9","aa","ab","ac","ad","ae","af","b0","b1","b2","b3","b4","b5","b6","b7","b8","b9","ba","bb","bc","bd","be","bf","c0","c1","c2","c3","c4","c5","c6","c7","c8","c9","ca","cb","cc","cd","ce","cf","d0","d1","d2","d3","d4","d5","d6","d7","d8","d9","da","db","dc","dd","de","df","e0","e1","e2","e3","e4","e5","e6","e7","e8","e9","ea","eb","ec","ed","ee","ef","f0","f1","f2","f3","f4","f5","f6","f7","f8","f9","fa","fb","fc","fd","fe","ff"];let ev=1234567;const vl=Math.PI/180,Tl=180/Math.PI;function Ca(){const r=Math.random()*4294967295|0,t=Math.random()*4294967295|0,n=Math.random()*4294967295|0,a=Math.random()*4294967295|0;return(Fn[r&255]+Fn[r>>8&255]+Fn[r>>16&255]+Fn[r>>24&255]+"-"+Fn[t&255]+Fn[t>>8&255]+"-"+Fn[t>>16&15|64]+Fn[t>>24&255]+"-"+Fn[n&63|128]+Fn[n>>8&255]+"-"+Fn[n>>16&255]+Fn[n>>24&255]+Fn[a&255]+Fn[a>>8&255]+Fn[a>>16&255]+Fn[a>>24&255]).toLowerCase()}function Se(r,t,n){return Math.max(t,Math.min(n,r))}function Xp(r,t){return(r%t+t)%t}function pb(r,t,n,a,l){return a+(r-t)*(l-a)/(n-t)}function mb(r,t,n){return r!==t?(n-r)/(t-r):0}function xl(r,t,n){return(1-n)*r+n*t}function gb(r,t,n,a){return xl(r,t,1-Math.exp(-n*a))}function _b(r,t=1){return t-Math.abs(Xp(r,t*2)-t)}function vb(r,t,n){return r<=t?0:r>=n?1:(r=(r-t)/(n-t),r*r*(3-2*r))}function xb(r,t,n){return r<=t?0:r>=n?1:(r=(r-t)/(n-t),r*r*r*(r*(r*6-15)+10))}function yb(r,t){return r+Math.floor(Math.random()*(t-r+1))}function Sb(r,t){return r+Math.random()*(t-r)}function Mb(r){return r*(.5-Math.random())}function bb(r){r!==void 0&&(ev=r);let t=ev+=1831565813;return t=Math.imul(t^t>>>15,t|1),t^=t+Math.imul(t^t>>>7,t|61),((t^t>>>14)>>>0)/4294967296}function Eb(r){return r*vl}function Tb(r){return r*Tl}function Ab(r){return(r&r-1)===0&&r!==0}function wb(r){return Math.pow(2,Math.ceil(Math.log(r)/Math.LN2))}function Rb(r){return Math.pow(2,Math.floor(Math.log(r)/Math.LN2))}function Cb(r,t,n,a,l){const c=Math.cos,f=Math.sin,d=c(n/2),m=f(n/2),h=c((t+a)/2),g=f((t+a)/2),_=c((t-a)/2),v=f((t-a)/2),y=c((a-t)/2),E=f((a-t)/2);switch(l){case"XYX":r.set(d*g,m*_,m*v,d*h);break;case"YZY":r.set(m*v,d*g,m*_,d*h);break;case"ZXZ":r.set(m*_,m*v,d*g,d*h);break;case"XZX":r.set(d*g,m*E,m*y,d*h);break;case"YXY":r.set(m*y,d*g,m*E,d*h);break;case"ZYZ":r.set(m*E,m*y,d*g,d*h);break;default:ce("MathUtils: .setQuaternionFromProperEuler() encountered an unknown order: "+l)}}function Fi(r,t){switch(t.constructor){case Float32Array:return r;case Uint32Array:return r/4294967295;case Uint16Array:return r/65535;case Uint8Array:return r/255;case Int32Array:return Math.max(r/2147483647,-1);case Int16Array:return Math.max(r/32767,-1);case Int8Array:return Math.max(r/127,-1);default:throw new Error("Invalid component type.")}}function Xe(r,t){switch(t.constructor){case Float32Array:return r;case Uint32Array:return Math.round(r*4294967295);case Uint16Array:return Math.round(r*65535);case Uint8Array:return Math.round(r*255);case Int32Array:return Math.round(r*2147483647);case Int16Array:return Math.round(r*32767);case Int8Array:return Math.round(r*127);default:throw new Error("Invalid component type.")}}const Zi={DEG2RAD:vl,RAD2DEG:Tl,generateUUID:Ca,clamp:Se,euclideanModulo:Xp,mapLinear:pb,inverseLerp:mb,lerp:xl,damp:gb,pingpong:_b,smoothstep:vb,smootherstep:xb,randInt:yb,randFloat:Sb,randFloatSpread:Mb,seededRandom:bb,degToRad:Eb,radToDeg:Tb,isPowerOfTwo:Ab,ceilPowerOfTwo:wb,floorPowerOfTwo:Rb,setQuaternionFromProperEuler:Cb,normalize:Xe,denormalize:Fi};class ee{static{ee.prototype.isVector2=!0}constructor(t=0,n=0){this.x=t,this.y=n}get width(){return this.x}set width(t){this.x=t}get height(){return this.y}set height(t){this.y=t}set(t,n){return this.x=t,this.y=n,this}setScalar(t){return this.x=t,this.y=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setComponent(t,n){switch(t){case 0:this.x=n;break;case 1:this.y=n;break;default:throw new Error("index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;default:throw new Error("index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y)}copy(t){return this.x=t.x,this.y=t.y,this}add(t){return this.x+=t.x,this.y+=t.y,this}addScalar(t){return this.x+=t,this.y+=t,this}addVectors(t,n){return this.x=t.x+n.x,this.y=t.y+n.y,this}addScaledVector(t,n){return this.x+=t.x*n,this.y+=t.y*n,this}sub(t){return this.x-=t.x,this.y-=t.y,this}subScalar(t){return this.x-=t,this.y-=t,this}subVectors(t,n){return this.x=t.x-n.x,this.y=t.y-n.y,this}multiply(t){return this.x*=t.x,this.y*=t.y,this}multiplyScalar(t){return this.x*=t,this.y*=t,this}divide(t){return this.x/=t.x,this.y/=t.y,this}divideScalar(t){return this.multiplyScalar(1/t)}applyMatrix3(t){const n=this.x,a=this.y,l=t.elements;return this.x=l[0]*n+l[3]*a+l[6],this.y=l[1]*n+l[4]*a+l[7],this}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this}clamp(t,n){return this.x=Se(this.x,t.x,n.x),this.y=Se(this.y,t.y,n.y),this}clampScalar(t,n){return this.x=Se(this.x,t,n),this.y=Se(this.y,t,n),this}clampLength(t,n){const a=this.length();return this.divideScalar(a||1).multiplyScalar(Se(a,t,n))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this}negate(){return this.x=-this.x,this.y=-this.y,this}dot(t){return this.x*t.x+this.y*t.y}cross(t){return this.x*t.y-this.y*t.x}lengthSq(){return this.x*this.x+this.y*this.y}length(){return Math.sqrt(this.x*this.x+this.y*this.y)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)}normalize(){return this.divideScalar(this.length()||1)}angle(){return Math.atan2(-this.y,-this.x)+Math.PI}angleTo(t){const n=Math.sqrt(this.lengthSq()*t.lengthSq());if(n===0)return Math.PI/2;const a=this.dot(t)/n;return Math.acos(Se(a,-1,1))}distanceTo(t){return Math.sqrt(this.distanceToSquared(t))}distanceToSquared(t){const n=this.x-t.x,a=this.y-t.y;return n*n+a*a}manhattanDistanceTo(t){return Math.abs(this.x-t.x)+Math.abs(this.y-t.y)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,n){return this.x+=(t.x-this.x)*n,this.y+=(t.y-this.y)*n,this}lerpVectors(t,n,a){return this.x=t.x+(n.x-t.x)*a,this.y=t.y+(n.y-t.y)*a,this}equals(t){return t.x===this.x&&t.y===this.y}fromArray(t,n=0){return this.x=t[n],this.y=t[n+1],this}toArray(t=[],n=0){return t[n]=this.x,t[n+1]=this.y,t}fromBufferAttribute(t,n){return this.x=t.getX(n),this.y=t.getY(n),this}rotateAround(t,n){const a=Math.cos(n),l=Math.sin(n),c=this.x-t.x,f=this.y-t.y;return this.x=c*a-f*l+t.x,this.y=c*l+f*a+t.y,this}random(){return this.x=Math.random(),this.y=Math.random(),this}*[Symbol.iterator](){yield this.x,yield this.y}}class wi{constructor(t=0,n=0,a=0,l=1){this.isQuaternion=!0,this._x=t,this._y=n,this._z=a,this._w=l}static slerpFlat(t,n,a,l,c,f,d){let m=a[l+0],h=a[l+1],g=a[l+2],_=a[l+3],v=c[f+0],y=c[f+1],E=c[f+2],A=c[f+3];if(_!==A||m!==v||h!==y||g!==E){let S=m*v+h*y+g*E+_*A;S<0&&(v=-v,y=-y,E=-E,A=-A,S=-S);let x=1-d;if(S<.9995){const w=Math.acos(S),D=Math.sin(w);x=Math.sin(x*w)/D,d=Math.sin(d*w)/D,m=m*x+v*d,h=h*x+y*d,g=g*x+E*d,_=_*x+A*d}else{m=m*x+v*d,h=h*x+y*d,g=g*x+E*d,_=_*x+A*d;const w=1/Math.sqrt(m*m+h*h+g*g+_*_);m*=w,h*=w,g*=w,_*=w}}t[n]=m,t[n+1]=h,t[n+2]=g,t[n+3]=_}static multiplyQuaternionsFlat(t,n,a,l,c,f){const d=a[l],m=a[l+1],h=a[l+2],g=a[l+3],_=c[f],v=c[f+1],y=c[f+2],E=c[f+3];return t[n]=d*E+g*_+m*y-h*v,t[n+1]=m*E+g*v+h*_-d*y,t[n+2]=h*E+g*y+d*v-m*_,t[n+3]=g*E-d*_-m*v-h*y,t}get x(){return this._x}set x(t){this._x=t,this._onChangeCallback()}get y(){return this._y}set y(t){this._y=t,this._onChangeCallback()}get z(){return this._z}set z(t){this._z=t,this._onChangeCallback()}get w(){return this._w}set w(t){this._w=t,this._onChangeCallback()}set(t,n,a,l){return this._x=t,this._y=n,this._z=a,this._w=l,this._onChangeCallback(),this}clone(){return new this.constructor(this._x,this._y,this._z,this._w)}copy(t){return this._x=t.x,this._y=t.y,this._z=t.z,this._w=t.w,this._onChangeCallback(),this}setFromEuler(t,n=!0){const a=t._x,l=t._y,c=t._z,f=t._order,d=Math.cos,m=Math.sin,h=d(a/2),g=d(l/2),_=d(c/2),v=m(a/2),y=m(l/2),E=m(c/2);switch(f){case"XYZ":this._x=v*g*_+h*y*E,this._y=h*y*_-v*g*E,this._z=h*g*E+v*y*_,this._w=h*g*_-v*y*E;break;case"YXZ":this._x=v*g*_+h*y*E,this._y=h*y*_-v*g*E,this._z=h*g*E-v*y*_,this._w=h*g*_+v*y*E;break;case"ZXY":this._x=v*g*_-h*y*E,this._y=h*y*_+v*g*E,this._z=h*g*E+v*y*_,this._w=h*g*_-v*y*E;break;case"ZYX":this._x=v*g*_-h*y*E,this._y=h*y*_+v*g*E,this._z=h*g*E-v*y*_,this._w=h*g*_+v*y*E;break;case"YZX":this._x=v*g*_+h*y*E,this._y=h*y*_+v*g*E,this._z=h*g*E-v*y*_,this._w=h*g*_-v*y*E;break;case"XZY":this._x=v*g*_-h*y*E,this._y=h*y*_-v*g*E,this._z=h*g*E+v*y*_,this._w=h*g*_+v*y*E;break;default:ce("Quaternion: .setFromEuler() encountered an unknown order: "+f)}return n===!0&&this._onChangeCallback(),this}setFromAxisAngle(t,n){const a=n/2,l=Math.sin(a);return this._x=t.x*l,this._y=t.y*l,this._z=t.z*l,this._w=Math.cos(a),this._onChangeCallback(),this}setFromRotationMatrix(t){const n=t.elements,a=n[0],l=n[4],c=n[8],f=n[1],d=n[5],m=n[9],h=n[2],g=n[6],_=n[10],v=a+d+_;if(v>0){const y=.5/Math.sqrt(v+1);this._w=.25/y,this._x=(g-m)*y,this._y=(c-h)*y,this._z=(f-l)*y}else if(a>d&&a>_){const y=2*Math.sqrt(1+a-d-_);this._w=(g-m)/y,this._x=.25*y,this._y=(l+f)/y,this._z=(c+h)/y}else if(d>_){const y=2*Math.sqrt(1+d-a-_);this._w=(c-h)/y,this._x=(l+f)/y,this._y=.25*y,this._z=(m+g)/y}else{const y=2*Math.sqrt(1+_-a-d);this._w=(f-l)/y,this._x=(c+h)/y,this._y=(m+g)/y,this._z=.25*y}return this._onChangeCallback(),this}setFromUnitVectors(t,n){let a=t.dot(n)+1;return a<1e-8?(a=0,Math.abs(t.x)>Math.abs(t.z)?(this._x=-t.y,this._y=t.x,this._z=0,this._w=a):(this._x=0,this._y=-t.z,this._z=t.y,this._w=a)):(this._x=t.y*n.z-t.z*n.y,this._y=t.z*n.x-t.x*n.z,this._z=t.x*n.y-t.y*n.x,this._w=a),this.normalize()}angleTo(t){return 2*Math.acos(Math.abs(Se(this.dot(t),-1,1)))}rotateTowards(t,n){const a=this.angleTo(t);if(a===0)return this;const l=Math.min(1,n/a);return this.slerp(t,l),this}identity(){return this.set(0,0,0,1)}invert(){return this.conjugate()}conjugate(){return this._x*=-1,this._y*=-1,this._z*=-1,this._onChangeCallback(),this}dot(t){return this._x*t._x+this._y*t._y+this._z*t._z+this._w*t._w}lengthSq(){return this._x*this._x+this._y*this._y+this._z*this._z+this._w*this._w}length(){return Math.sqrt(this._x*this._x+this._y*this._y+this._z*this._z+this._w*this._w)}normalize(){let t=this.length();return t===0?(this._x=0,this._y=0,this._z=0,this._w=1):(t=1/t,this._x=this._x*t,this._y=this._y*t,this._z=this._z*t,this._w=this._w*t),this._onChangeCallback(),this}multiply(t){return this.multiplyQuaternions(this,t)}premultiply(t){return this.multiplyQuaternions(t,this)}multiplyQuaternions(t,n){const a=t._x,l=t._y,c=t._z,f=t._w,d=n._x,m=n._y,h=n._z,g=n._w;return this._x=a*g+f*d+l*h-c*m,this._y=l*g+f*m+c*d-a*h,this._z=c*g+f*h+a*m-l*d,this._w=f*g-a*d-l*m-c*h,this._onChangeCallback(),this}slerp(t,n){let a=t._x,l=t._y,c=t._z,f=t._w,d=this.dot(t);d<0&&(a=-a,l=-l,c=-c,f=-f,d=-d);let m=1-n;if(d<.9995){const h=Math.acos(d),g=Math.sin(h);m=Math.sin(m*h)/g,n=Math.sin(n*h)/g,this._x=this._x*m+a*n,this._y=this._y*m+l*n,this._z=this._z*m+c*n,this._w=this._w*m+f*n,this._onChangeCallback()}else this._x=this._x*m+a*n,this._y=this._y*m+l*n,this._z=this._z*m+c*n,this._w=this._w*m+f*n,this.normalize();return this}slerpQuaternions(t,n,a){return this.copy(t).slerp(n,a)}random(){const t=2*Math.PI*Math.random(),n=2*Math.PI*Math.random(),a=Math.random(),l=Math.sqrt(1-a),c=Math.sqrt(a);return this.set(l*Math.sin(t),l*Math.cos(t),c*Math.sin(n),c*Math.cos(n))}equals(t){return t._x===this._x&&t._y===this._y&&t._z===this._z&&t._w===this._w}fromArray(t,n=0){return this._x=t[n],this._y=t[n+1],this._z=t[n+2],this._w=t[n+3],this._onChangeCallback(),this}toArray(t=[],n=0){return t[n]=this._x,t[n+1]=this._y,t[n+2]=this._z,t[n+3]=this._w,t}fromBufferAttribute(t,n){return this._x=t.getX(n),this._y=t.getY(n),this._z=t.getZ(n),this._w=t.getW(n),this._onChangeCallback(),this}toJSON(){return this.toArray()}_onChange(t){return this._onChangeCallback=t,this}_onChangeCallback(){}*[Symbol.iterator](){yield this._x,yield this._y,yield this._z,yield this._w}}class k{static{k.prototype.isVector3=!0}constructor(t=0,n=0,a=0){this.x=t,this.y=n,this.z=a}set(t,n,a){return a===void 0&&(a=this.z),this.x=t,this.y=n,this.z=a,this}setScalar(t){return this.x=t,this.y=t,this.z=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setZ(t){return this.z=t,this}setComponent(t,n){switch(t){case 0:this.x=n;break;case 1:this.y=n;break;case 2:this.z=n;break;default:throw new Error("index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;case 2:return this.z;default:throw new Error("index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y,this.z)}copy(t){return this.x=t.x,this.y=t.y,this.z=t.z,this}add(t){return this.x+=t.x,this.y+=t.y,this.z+=t.z,this}addScalar(t){return this.x+=t,this.y+=t,this.z+=t,this}addVectors(t,n){return this.x=t.x+n.x,this.y=t.y+n.y,this.z=t.z+n.z,this}addScaledVector(t,n){return this.x+=t.x*n,this.y+=t.y*n,this.z+=t.z*n,this}sub(t){return this.x-=t.x,this.y-=t.y,this.z-=t.z,this}subScalar(t){return this.x-=t,this.y-=t,this.z-=t,this}subVectors(t,n){return this.x=t.x-n.x,this.y=t.y-n.y,this.z=t.z-n.z,this}multiply(t){return this.x*=t.x,this.y*=t.y,this.z*=t.z,this}multiplyScalar(t){return this.x*=t,this.y*=t,this.z*=t,this}multiplyVectors(t,n){return this.x=t.x*n.x,this.y=t.y*n.y,this.z=t.z*n.z,this}applyEuler(t){return this.applyQuaternion(nv.setFromEuler(t))}applyAxisAngle(t,n){return this.applyQuaternion(nv.setFromAxisAngle(t,n))}applyMatrix3(t){const n=this.x,a=this.y,l=this.z,c=t.elements;return this.x=c[0]*n+c[3]*a+c[6]*l,this.y=c[1]*n+c[4]*a+c[7]*l,this.z=c[2]*n+c[5]*a+c[8]*l,this}applyNormalMatrix(t){return this.applyMatrix3(t).normalize()}applyMatrix4(t){const n=this.x,a=this.y,l=this.z,c=t.elements,f=1/(c[3]*n+c[7]*a+c[11]*l+c[15]);return this.x=(c[0]*n+c[4]*a+c[8]*l+c[12])*f,this.y=(c[1]*n+c[5]*a+c[9]*l+c[13])*f,this.z=(c[2]*n+c[6]*a+c[10]*l+c[14])*f,this}applyQuaternion(t){const n=this.x,a=this.y,l=this.z,c=t.x,f=t.y,d=t.z,m=t.w,h=2*(f*l-d*a),g=2*(d*n-c*l),_=2*(c*a-f*n);return this.x=n+m*h+f*_-d*g,this.y=a+m*g+d*h-c*_,this.z=l+m*_+c*g-f*h,this}project(t){return this.applyMatrix4(t.matrixWorldInverse).applyMatrix4(t.projectionMatrix)}unproject(t){return this.applyMatrix4(t.projectionMatrixInverse).applyMatrix4(t.matrixWorld)}transformDirection(t){const n=this.x,a=this.y,l=this.z,c=t.elements;return this.x=c[0]*n+c[4]*a+c[8]*l,this.y=c[1]*n+c[5]*a+c[9]*l,this.z=c[2]*n+c[6]*a+c[10]*l,this.normalize()}divide(t){return this.x/=t.x,this.y/=t.y,this.z/=t.z,this}divideScalar(t){return this.multiplyScalar(1/t)}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this.z=Math.min(this.z,t.z),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this.z=Math.max(this.z,t.z),this}clamp(t,n){return this.x=Se(this.x,t.x,n.x),this.y=Se(this.y,t.y,n.y),this.z=Se(this.z,t.z,n.z),this}clampScalar(t,n){return this.x=Se(this.x,t,n),this.y=Se(this.y,t,n),this.z=Se(this.z,t,n),this}clampLength(t,n){const a=this.length();return this.divideScalar(a||1).multiplyScalar(Se(a,t,n))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this.z=Math.floor(this.z),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this.z=Math.ceil(this.z),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this.z=Math.round(this.z),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this.z=Math.trunc(this.z),this}negate(){return this.x=-this.x,this.y=-this.y,this.z=-this.z,this}dot(t){return this.x*t.x+this.y*t.y+this.z*t.z}lengthSq(){return this.x*this.x+this.y*this.y+this.z*this.z}length(){return Math.sqrt(this.x*this.x+this.y*this.y+this.z*this.z)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)+Math.abs(this.z)}normalize(){return this.divideScalar(this.length()||1)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,n){return this.x+=(t.x-this.x)*n,this.y+=(t.y-this.y)*n,this.z+=(t.z-this.z)*n,this}lerpVectors(t,n,a){return this.x=t.x+(n.x-t.x)*a,this.y=t.y+(n.y-t.y)*a,this.z=t.z+(n.z-t.z)*a,this}cross(t){return this.crossVectors(this,t)}crossVectors(t,n){const a=t.x,l=t.y,c=t.z,f=n.x,d=n.y,m=n.z;return this.x=l*m-c*d,this.y=c*f-a*m,this.z=a*d-l*f,this}projectOnVector(t){const n=t.lengthSq();if(n===0)return this.set(0,0,0);const a=t.dot(this)/n;return this.copy(t).multiplyScalar(a)}projectOnPlane(t){return th.copy(this).projectOnVector(t),this.sub(th)}reflect(t){return this.sub(th.copy(t).multiplyScalar(2*this.dot(t)))}angleTo(t){const n=Math.sqrt(this.lengthSq()*t.lengthSq());if(n===0)return Math.PI/2;const a=this.dot(t)/n;return Math.acos(Se(a,-1,1))}distanceTo(t){return Math.sqrt(this.distanceToSquared(t))}distanceToSquared(t){const n=this.x-t.x,a=this.y-t.y,l=this.z-t.z;return n*n+a*a+l*l}manhattanDistanceTo(t){return Math.abs(this.x-t.x)+Math.abs(this.y-t.y)+Math.abs(this.z-t.z)}setFromSpherical(t){return this.setFromSphericalCoords(t.radius,t.phi,t.theta)}setFromSphericalCoords(t,n,a){const l=Math.sin(n)*t;return this.x=l*Math.sin(a),this.y=Math.cos(n)*t,this.z=l*Math.cos(a),this}setFromCylindrical(t){return this.setFromCylindricalCoords(t.radius,t.theta,t.y)}setFromCylindricalCoords(t,n,a){return this.x=t*Math.sin(n),this.y=a,this.z=t*Math.cos(n),this}setFromMatrixPosition(t){const n=t.elements;return this.x=n[12],this.y=n[13],this.z=n[14],this}setFromMatrixScale(t){const n=this.setFromMatrixColumn(t,0).length(),a=this.setFromMatrixColumn(t,1).length(),l=this.setFromMatrixColumn(t,2).length();return this.x=n,this.y=a,this.z=l,this}setFromMatrixColumn(t,n){return this.fromArray(t.elements,n*4)}setFromMatrix3Column(t,n){return this.fromArray(t.elements,n*3)}setFromEuler(t){return this.x=t._x,this.y=t._y,this.z=t._z,this}setFromColor(t){return this.x=t.r,this.y=t.g,this.z=t.b,this}equals(t){return t.x===this.x&&t.y===this.y&&t.z===this.z}fromArray(t,n=0){return this.x=t[n],this.y=t[n+1],this.z=t[n+2],this}toArray(t=[],n=0){return t[n]=this.x,t[n+1]=this.y,t[n+2]=this.z,t}fromBufferAttribute(t,n){return this.x=t.getX(n),this.y=t.getY(n),this.z=t.getZ(n),this}random(){return this.x=Math.random(),this.y=Math.random(),this.z=Math.random(),this}randomDirection(){const t=Math.random()*Math.PI*2,n=Math.random()*2-1,a=Math.sqrt(1-n*n);return this.x=a*Math.cos(t),this.y=n,this.z=a*Math.sin(t),this}*[Symbol.iterator](){yield this.x,yield this.y,yield this.z}}const th=new k,nv=new wi;class pe{static{pe.prototype.isMatrix3=!0}constructor(t,n,a,l,c,f,d,m,h){this.elements=[1,0,0,0,1,0,0,0,1],t!==void 0&&this.set(t,n,a,l,c,f,d,m,h)}set(t,n,a,l,c,f,d,m,h){const g=this.elements;return g[0]=t,g[1]=l,g[2]=d,g[3]=n,g[4]=c,g[5]=m,g[6]=a,g[7]=f,g[8]=h,this}identity(){return this.set(1,0,0,0,1,0,0,0,1),this}copy(t){const n=this.elements,a=t.elements;return n[0]=a[0],n[1]=a[1],n[2]=a[2],n[3]=a[3],n[4]=a[4],n[5]=a[5],n[6]=a[6],n[7]=a[7],n[8]=a[8],this}extractBasis(t,n,a){return t.setFromMatrix3Column(this,0),n.setFromMatrix3Column(this,1),a.setFromMatrix3Column(this,2),this}setFromMatrix4(t){const n=t.elements;return this.set(n[0],n[4],n[8],n[1],n[5],n[9],n[2],n[6],n[10]),this}multiply(t){return this.multiplyMatrices(this,t)}premultiply(t){return this.multiplyMatrices(t,this)}multiplyMatrices(t,n){const a=t.elements,l=n.elements,c=this.elements,f=a[0],d=a[3],m=a[6],h=a[1],g=a[4],_=a[7],v=a[2],y=a[5],E=a[8],A=l[0],S=l[3],x=l[6],w=l[1],D=l[4],U=l[7],G=l[2],O=l[5],B=l[8];return c[0]=f*A+d*w+m*G,c[3]=f*S+d*D+m*O,c[6]=f*x+d*U+m*B,c[1]=h*A+g*w+_*G,c[4]=h*S+g*D+_*O,c[7]=h*x+g*U+_*B,c[2]=v*A+y*w+E*G,c[5]=v*S+y*D+E*O,c[8]=v*x+y*U+E*B,this}multiplyScalar(t){const n=this.elements;return n[0]*=t,n[3]*=t,n[6]*=t,n[1]*=t,n[4]*=t,n[7]*=t,n[2]*=t,n[5]*=t,n[8]*=t,this}determinant(){const t=this.elements,n=t[0],a=t[1],l=t[2],c=t[3],f=t[4],d=t[5],m=t[6],h=t[7],g=t[8];return n*f*g-n*d*h-a*c*g+a*d*m+l*c*h-l*f*m}invert(){const t=this.elements,n=t[0],a=t[1],l=t[2],c=t[3],f=t[4],d=t[5],m=t[6],h=t[7],g=t[8],_=g*f-d*h,v=d*m-g*c,y=h*c-f*m,E=n*_+a*v+l*y;if(E===0)return this.set(0,0,0,0,0,0,0,0,0);const A=1/E;return t[0]=_*A,t[1]=(l*h-g*a)*A,t[2]=(d*a-l*f)*A,t[3]=v*A,t[4]=(g*n-l*m)*A,t[5]=(l*c-d*n)*A,t[6]=y*A,t[7]=(a*m-h*n)*A,t[8]=(f*n-a*c)*A,this}transpose(){let t;const n=this.elements;return t=n[1],n[1]=n[3],n[3]=t,t=n[2],n[2]=n[6],n[6]=t,t=n[5],n[5]=n[7],n[7]=t,this}getNormalMatrix(t){return this.setFromMatrix4(t).invert().transpose()}transposeIntoArray(t){const n=this.elements;return t[0]=n[0],t[1]=n[3],t[2]=n[6],t[3]=n[1],t[4]=n[4],t[5]=n[7],t[6]=n[2],t[7]=n[5],t[8]=n[8],this}setUvTransform(t,n,a,l,c,f,d){const m=Math.cos(c),h=Math.sin(c);return this.set(a*m,a*h,-a*(m*f+h*d)+f+t,-l*h,l*m,-l*(-h*f+m*d)+d+n,0,0,1),this}scale(t,n){return this.premultiply(eh.makeScale(t,n)),this}rotate(t){return this.premultiply(eh.makeRotation(-t)),this}translate(t,n){return this.premultiply(eh.makeTranslation(t,n)),this}makeTranslation(t,n){return t.isVector2?this.set(1,0,t.x,0,1,t.y,0,0,1):this.set(1,0,t,0,1,n,0,0,1),this}makeRotation(t){const n=Math.cos(t),a=Math.sin(t);return this.set(n,-a,0,a,n,0,0,0,1),this}makeScale(t,n){return this.set(t,0,0,0,n,0,0,0,1),this}equals(t){const n=this.elements,a=t.elements;for(let l=0;l<9;l++)if(n[l]!==a[l])return!1;return!0}fromArray(t,n=0){for(let a=0;a<9;a++)this.elements[a]=t[a+n];return this}toArray(t=[],n=0){const a=this.elements;return t[n]=a[0],t[n+1]=a[1],t[n+2]=a[2],t[n+3]=a[3],t[n+4]=a[4],t[n+5]=a[5],t[n+6]=a[6],t[n+7]=a[7],t[n+8]=a[8],t}clone(){return new this.constructor().fromArray(this.elements)}}const eh=new pe,iv=new pe().set(.4123908,.3575843,.1804808,.212639,.7151687,.0721923,.0193308,.1191948,.9505322),av=new pe().set(3.2409699,-1.5373832,-.4986108,-.9692436,1.8759675,.0415551,.0556301,-.203977,1.0569715);function Nb(){const r={enabled:!0,workingColorSpace:Eu,spaces:{},convert:function(l,c,f){return this.enabled===!1||c===f||!c||!f||(this.spaces[c].transfer===je&&(l.r=Na(l.r),l.g=Na(l.g),l.b=Na(l.b)),this.spaces[c].primaries!==this.spaces[f].primaries&&(l.applyMatrix3(this.spaces[c].toXYZ),l.applyMatrix3(this.spaces[f].fromXYZ)),this.spaces[f].transfer===je&&(l.r=ao(l.r),l.g=ao(l.g),l.b=ao(l.b))),l},workingToColorSpace:function(l,c){return this.convert(l,this.workingColorSpace,c)},colorSpaceToWorking:function(l,c){return this.convert(l,c,this.workingColorSpace)},getPrimaries:function(l){return this.spaces[l].primaries},getTransfer:function(l){return l===gs?Tu:this.spaces[l].transfer},getToneMappingMode:function(l){return this.spaces[l].outputColorSpaceConfig.toneMappingMode||"standard"},getLuminanceCoefficients:function(l,c=this.workingColorSpace){return l.fromArray(this.spaces[c].luminanceCoefficients)},define:function(l){Object.assign(this.spaces,l)},_getMatrix:function(l,c,f){return l.copy(this.spaces[c].toXYZ).multiply(this.spaces[f].fromXYZ)},_getDrawingBufferColorSpace:function(l){return this.spaces[l].outputColorSpaceConfig.drawingBufferColorSpace},_getUnpackColorSpace:function(l=this.workingColorSpace){return this.spaces[l].workingColorSpaceConfig.unpackColorSpace},fromWorkingColorSpace:function(l,c){return Tp("ColorManagement: .fromWorkingColorSpace() has been renamed to .workingToColorSpace()."),r.workingToColorSpace(l,c)},toWorkingColorSpace:function(l,c){return Tp("ColorManagement: .toWorkingColorSpace() has been renamed to .colorSpaceToWorking()."),r.colorSpaceToWorking(l,c)}},t=[.64,.33,.3,.6,.15,.06],n=[.2126,.7152,.0722],a=[.3127,.329];return r.define({[Eu]:{primaries:t,whitePoint:a,transfer:Tu,toXYZ:iv,fromXYZ:av,luminanceCoefficients:n,workingColorSpaceConfig:{unpackColorSpace:Ti},outputColorSpaceConfig:{drawingBufferColorSpace:Ti}},[Ti]:{primaries:t,whitePoint:a,transfer:je,toXYZ:iv,fromXYZ:av,luminanceCoefficients:n,outputColorSpaceConfig:{drawingBufferColorSpace:Ti}}}),r}const De=Nb();function Na(r){return r<.04045?r*.0773993808:Math.pow(r*.9478672986+.0521327014,2.4)}function ao(r){return r<.0031308?r*12.92:1.055*Math.pow(r,.41666)-.055}let Pr;class Db{static getDataURL(t,n="image/png"){if(/^data:/i.test(t.src)||typeof HTMLCanvasElement>"u")return t.src;let a;if(t instanceof HTMLCanvasElement)a=t;else{Pr===void 0&&(Pr=Au("canvas")),Pr.width=t.width,Pr.height=t.height;const l=Pr.getContext("2d");t instanceof ImageData?l.putImageData(t,0,0):l.drawImage(t,0,0,t.width,t.height),a=Pr}return a.toDataURL(n)}static sRGBToLinear(t){if(typeof HTMLImageElement<"u"&&t instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&t instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&t instanceof ImageBitmap){const n=Au("canvas");n.width=t.width,n.height=t.height;const a=n.getContext("2d");a.drawImage(t,0,0,t.width,t.height);const l=a.getImageData(0,0,t.width,t.height),c=l.data;for(let f=0;f<c.length;f++)c[f]=Na(c[f]/255)*255;return a.putImageData(l,0,0),n}else if(t.data){const n=t.data.slice(0);for(let a=0;a<n.length;a++)n instanceof Uint8Array||n instanceof Uint8ClampedArray?n[a]=Math.floor(Na(n[a]/255)*255):n[a]=Na(n[a]);return{data:n,width:t.width,height:t.height}}else return ce("ImageUtils.sRGBToLinear(): Unsupported image type. No color space conversion applied."),t}}let Ub=0;class Wp{constructor(t=null){this.isSource=!0,Object.defineProperty(this,"id",{value:Ub++}),this.uuid=Ca(),this.data=t,this.dataReady=!0,this.version=0}getSize(t){const n=this.data;return typeof HTMLVideoElement<"u"&&n instanceof HTMLVideoElement?t.set(n.videoWidth,n.videoHeight,0):typeof VideoFrame<"u"&&n instanceof VideoFrame?t.set(n.displayWidth,n.displayHeight,0):n!==null?t.set(n.width,n.height,n.depth||0):t.set(0,0,0),t}set needsUpdate(t){t===!0&&this.version++}toJSON(t){const n=t===void 0||typeof t=="string";if(!n&&t.images[this.uuid]!==void 0)return t.images[this.uuid];const a={uuid:this.uuid,url:""},l=this.data;if(l!==null){let c;if(Array.isArray(l)){c=[];for(let f=0,d=l.length;f<d;f++)l[f].isDataTexture?c.push(nh(l[f].image)):c.push(nh(l[f]))}else c=nh(l);a.url=c}return n||(t.images[this.uuid]=a),a}}function nh(r){return typeof HTMLImageElement<"u"&&r instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&r instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&r instanceof ImageBitmap?Db.getDataURL(r):r.data?{data:Array.from(r.data),width:r.width,height:r.height,type:r.data.constructor.name}:(ce("Texture: Unable to serialize Texture."),{})}let Lb=0;const ih=new k;class Vn extends tr{constructor(t=Vn.DEFAULT_IMAGE,n=Vn.DEFAULT_MAPPING,a=wa,l=wa,c=Sn,f=Zs,d=Hi,m=pi,h=Vn.DEFAULT_ANISOTROPY,g=gs){super(),this.isTexture=!0,Object.defineProperty(this,"id",{value:Lb++}),this.uuid=Ca(),this.name="",this.source=new Wp(t),this.mipmaps=[],this.mapping=n,this.channel=0,this.wrapS=a,this.wrapT=l,this.magFilter=c,this.minFilter=f,this.anisotropy=h,this.format=d,this.internalFormat=null,this.type=m,this.offset=new ee(0,0),this.repeat=new ee(1,1),this.center=new ee(0,0),this.rotation=0,this.matrixAutoUpdate=!0,this.matrix=new pe,this.generateMipmaps=!0,this.premultiplyAlpha=!1,this.flipY=!0,this.unpackAlignment=4,this.colorSpace=g,this.userData={},this.updateRanges=[],this.version=0,this.onUpdate=null,this.renderTarget=null,this.isRenderTargetTexture=!1,this.isArrayTexture=!!(t&&t.depth&&t.depth>1),this.pmremVersion=0,this.normalized=!1}get width(){return this.source.getSize(ih).x}get height(){return this.source.getSize(ih).y}get depth(){return this.source.getSize(ih).z}get image(){return this.source.data}set image(t){this.source.data=t}updateMatrix(){this.matrix.setUvTransform(this.offset.x,this.offset.y,this.repeat.x,this.repeat.y,this.rotation,this.center.x,this.center.y)}addUpdateRange(t,n){this.updateRanges.push({start:t,count:n})}clearUpdateRanges(){this.updateRanges.length=0}clone(){return new this.constructor().copy(this)}copy(t){return this.name=t.name,this.source=t.source,this.mipmaps=t.mipmaps.slice(0),this.mapping=t.mapping,this.channel=t.channel,this.wrapS=t.wrapS,this.wrapT=t.wrapT,this.magFilter=t.magFilter,this.minFilter=t.minFilter,this.anisotropy=t.anisotropy,this.format=t.format,this.internalFormat=t.internalFormat,this.type=t.type,this.normalized=t.normalized,this.offset.copy(t.offset),this.repeat.copy(t.repeat),this.center.copy(t.center),this.rotation=t.rotation,this.matrixAutoUpdate=t.matrixAutoUpdate,this.matrix.copy(t.matrix),this.generateMipmaps=t.generateMipmaps,this.premultiplyAlpha=t.premultiplyAlpha,this.flipY=t.flipY,this.unpackAlignment=t.unpackAlignment,this.colorSpace=t.colorSpace,this.renderTarget=t.renderTarget,this.isRenderTargetTexture=t.isRenderTargetTexture,this.isArrayTexture=t.isArrayTexture,this.userData=JSON.parse(JSON.stringify(t.userData)),this.needsUpdate=!0,this}setValues(t){for(const n in t){const a=t[n];if(a===void 0){ce(`Texture.setValues(): parameter '${n}' has value of undefined.`);continue}const l=this[n];if(l===void 0){ce(`Texture.setValues(): property '${n}' does not exist.`);continue}l&&a&&l.isVector2&&a.isVector2||l&&a&&l.isVector3&&a.isVector3||l&&a&&l.isMatrix3&&a.isMatrix3?l.copy(a):this[n]=a}}toJSON(t){const n=t===void 0||typeof t=="string";if(!n&&t.textures[this.uuid]!==void 0)return t.textures[this.uuid];const a={metadata:{version:4.7,type:"Texture",generator:"Texture.toJSON"},uuid:this.uuid,name:this.name,image:this.source.toJSON(t).uuid,mapping:this.mapping,channel:this.channel,repeat:[this.repeat.x,this.repeat.y],offset:[this.offset.x,this.offset.y],center:[this.center.x,this.center.y],rotation:this.rotation,wrap:[this.wrapS,this.wrapT],format:this.format,internalFormat:this.internalFormat,type:this.type,normalized:this.normalized,colorSpace:this.colorSpace,minFilter:this.minFilter,magFilter:this.magFilter,anisotropy:this.anisotropy,flipY:this.flipY,generateMipmaps:this.generateMipmaps,premultiplyAlpha:this.premultiplyAlpha,unpackAlignment:this.unpackAlignment};return Object.keys(this.userData).length>0&&(a.userData=this.userData),n||(t.textures[this.uuid]=a),a}dispose(){this.dispatchEvent({type:"dispose"})}transformUv(t){if(this.mapping!==Tx)return t;if(t.applyMatrix3(this.matrix),t.x<0||t.x>1)switch(this.wrapS){case Xh:t.x=t.x-Math.floor(t.x);break;case wa:t.x=t.x<0?0:1;break;case Wh:Math.abs(Math.floor(t.x)%2)===1?t.x=Math.ceil(t.x)-t.x:t.x=t.x-Math.floor(t.x);break}if(t.y<0||t.y>1)switch(this.wrapT){case Xh:t.y=t.y-Math.floor(t.y);break;case wa:t.y=t.y<0?0:1;break;case Wh:Math.abs(Math.floor(t.y)%2)===1?t.y=Math.ceil(t.y)-t.y:t.y=t.y-Math.floor(t.y);break}return this.flipY&&(t.y=1-t.y),t}set needsUpdate(t){t===!0&&(this.version++,this.source.needsUpdate=!0)}set needsPMREMUpdate(t){t===!0&&this.pmremVersion++}}Vn.DEFAULT_IMAGE=null;Vn.DEFAULT_MAPPING=Tx;Vn.DEFAULT_ANISOTROPY=1;class fn{static{fn.prototype.isVector4=!0}constructor(t=0,n=0,a=0,l=1){this.x=t,this.y=n,this.z=a,this.w=l}get width(){return this.z}set width(t){this.z=t}get height(){return this.w}set height(t){this.w=t}set(t,n,a,l){return this.x=t,this.y=n,this.z=a,this.w=l,this}setScalar(t){return this.x=t,this.y=t,this.z=t,this.w=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setZ(t){return this.z=t,this}setW(t){return this.w=t,this}setComponent(t,n){switch(t){case 0:this.x=n;break;case 1:this.y=n;break;case 2:this.z=n;break;case 3:this.w=n;break;default:throw new Error("index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;case 2:return this.z;case 3:return this.w;default:throw new Error("index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y,this.z,this.w)}copy(t){return this.x=t.x,this.y=t.y,this.z=t.z,this.w=t.w!==void 0?t.w:1,this}add(t){return this.x+=t.x,this.y+=t.y,this.z+=t.z,this.w+=t.w,this}addScalar(t){return this.x+=t,this.y+=t,this.z+=t,this.w+=t,this}addVectors(t,n){return this.x=t.x+n.x,this.y=t.y+n.y,this.z=t.z+n.z,this.w=t.w+n.w,this}addScaledVector(t,n){return this.x+=t.x*n,this.y+=t.y*n,this.z+=t.z*n,this.w+=t.w*n,this}sub(t){return this.x-=t.x,this.y-=t.y,this.z-=t.z,this.w-=t.w,this}subScalar(t){return this.x-=t,this.y-=t,this.z-=t,this.w-=t,this}subVectors(t,n){return this.x=t.x-n.x,this.y=t.y-n.y,this.z=t.z-n.z,this.w=t.w-n.w,this}multiply(t){return this.x*=t.x,this.y*=t.y,this.z*=t.z,this.w*=t.w,this}multiplyScalar(t){return this.x*=t,this.y*=t,this.z*=t,this.w*=t,this}applyMatrix4(t){const n=this.x,a=this.y,l=this.z,c=this.w,f=t.elements;return this.x=f[0]*n+f[4]*a+f[8]*l+f[12]*c,this.y=f[1]*n+f[5]*a+f[9]*l+f[13]*c,this.z=f[2]*n+f[6]*a+f[10]*l+f[14]*c,this.w=f[3]*n+f[7]*a+f[11]*l+f[15]*c,this}divide(t){return this.x/=t.x,this.y/=t.y,this.z/=t.z,this.w/=t.w,this}divideScalar(t){return this.multiplyScalar(1/t)}setAxisAngleFromQuaternion(t){this.w=2*Math.acos(t.w);const n=Math.sqrt(1-t.w*t.w);return n<1e-4?(this.x=1,this.y=0,this.z=0):(this.x=t.x/n,this.y=t.y/n,this.z=t.z/n),this}setAxisAngleFromRotationMatrix(t){let n,a,l,c;const m=t.elements,h=m[0],g=m[4],_=m[8],v=m[1],y=m[5],E=m[9],A=m[2],S=m[6],x=m[10];if(Math.abs(g-v)<.01&&Math.abs(_-A)<.01&&Math.abs(E-S)<.01){if(Math.abs(g+v)<.1&&Math.abs(_+A)<.1&&Math.abs(E+S)<.1&&Math.abs(h+y+x-3)<.1)return this.set(1,0,0,0),this;n=Math.PI;const D=(h+1)/2,U=(y+1)/2,G=(x+1)/2,O=(g+v)/4,B=(_+A)/4,R=(E+S)/4;return D>U&&D>G?D<.01?(a=0,l=.707106781,c=.707106781):(a=Math.sqrt(D),l=O/a,c=B/a):U>G?U<.01?(a=.707106781,l=0,c=.707106781):(l=Math.sqrt(U),a=O/l,c=R/l):G<.01?(a=.707106781,l=.707106781,c=0):(c=Math.sqrt(G),a=B/c,l=R/c),this.set(a,l,c,n),this}let w=Math.sqrt((S-E)*(S-E)+(_-A)*(_-A)+(v-g)*(v-g));return Math.abs(w)<.001&&(w=1),this.x=(S-E)/w,this.y=(_-A)/w,this.z=(v-g)/w,this.w=Math.acos((h+y+x-1)/2),this}setFromMatrixPosition(t){const n=t.elements;return this.x=n[12],this.y=n[13],this.z=n[14],this.w=n[15],this}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this.z=Math.min(this.z,t.z),this.w=Math.min(this.w,t.w),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this.z=Math.max(this.z,t.z),this.w=Math.max(this.w,t.w),this}clamp(t,n){return this.x=Se(this.x,t.x,n.x),this.y=Se(this.y,t.y,n.y),this.z=Se(this.z,t.z,n.z),this.w=Se(this.w,t.w,n.w),this}clampScalar(t,n){return this.x=Se(this.x,t,n),this.y=Se(this.y,t,n),this.z=Se(this.z,t,n),this.w=Se(this.w,t,n),this}clampLength(t,n){const a=this.length();return this.divideScalar(a||1).multiplyScalar(Se(a,t,n))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this.z=Math.floor(this.z),this.w=Math.floor(this.w),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this.z=Math.ceil(this.z),this.w=Math.ceil(this.w),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this.z=Math.round(this.z),this.w=Math.round(this.w),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this.z=Math.trunc(this.z),this.w=Math.trunc(this.w),this}negate(){return this.x=-this.x,this.y=-this.y,this.z=-this.z,this.w=-this.w,this}dot(t){return this.x*t.x+this.y*t.y+this.z*t.z+this.w*t.w}lengthSq(){return this.x*this.x+this.y*this.y+this.z*this.z+this.w*this.w}length(){return Math.sqrt(this.x*this.x+this.y*this.y+this.z*this.z+this.w*this.w)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)+Math.abs(this.z)+Math.abs(this.w)}normalize(){return this.divideScalar(this.length()||1)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,n){return this.x+=(t.x-this.x)*n,this.y+=(t.y-this.y)*n,this.z+=(t.z-this.z)*n,this.w+=(t.w-this.w)*n,this}lerpVectors(t,n,a){return this.x=t.x+(n.x-t.x)*a,this.y=t.y+(n.y-t.y)*a,this.z=t.z+(n.z-t.z)*a,this.w=t.w+(n.w-t.w)*a,this}equals(t){return t.x===this.x&&t.y===this.y&&t.z===this.z&&t.w===this.w}fromArray(t,n=0){return this.x=t[n],this.y=t[n+1],this.z=t[n+2],this.w=t[n+3],this}toArray(t=[],n=0){return t[n]=this.x,t[n+1]=this.y,t[n+2]=this.z,t[n+3]=this.w,t}fromBufferAttribute(t,n){return this.x=t.getX(n),this.y=t.getY(n),this.z=t.getZ(n),this.w=t.getW(n),this}random(){return this.x=Math.random(),this.y=Math.random(),this.z=Math.random(),this.w=Math.random(),this}*[Symbol.iterator](){yield this.x,yield this.y,yield this.z,yield this.w}}class Ob extends tr{constructor(t=1,n=1,a={}){super(),a=Object.assign({generateMipmaps:!1,internalFormat:null,minFilter:Sn,depthBuffer:!0,stencilBuffer:!1,resolveDepthBuffer:!0,resolveStencilBuffer:!0,depthTexture:null,samples:0,count:1,depth:1,multiview:!1},a),this.isRenderTarget=!0,this.width=t,this.height=n,this.depth=a.depth,this.scissor=new fn(0,0,t,n),this.scissorTest=!1,this.viewport=new fn(0,0,t,n),this.textures=[];const l={width:t,height:n,depth:a.depth},c=new Vn(l),f=a.count;for(let d=0;d<f;d++)this.textures[d]=c.clone(),this.textures[d].isRenderTargetTexture=!0,this.textures[d].renderTarget=this;this._setTextureOptions(a),this.depthBuffer=a.depthBuffer,this.stencilBuffer=a.stencilBuffer,this.resolveDepthBuffer=a.resolveDepthBuffer,this.resolveStencilBuffer=a.resolveStencilBuffer,this._depthTexture=null,this.depthTexture=a.depthTexture,this.samples=a.samples,this.multiview=a.multiview}_setTextureOptions(t={}){const n={minFilter:Sn,generateMipmaps:!1,flipY:!1,internalFormat:null};t.mapping!==void 0&&(n.mapping=t.mapping),t.wrapS!==void 0&&(n.wrapS=t.wrapS),t.wrapT!==void 0&&(n.wrapT=t.wrapT),t.wrapR!==void 0&&(n.wrapR=t.wrapR),t.magFilter!==void 0&&(n.magFilter=t.magFilter),t.minFilter!==void 0&&(n.minFilter=t.minFilter),t.format!==void 0&&(n.format=t.format),t.type!==void 0&&(n.type=t.type),t.anisotropy!==void 0&&(n.anisotropy=t.anisotropy),t.colorSpace!==void 0&&(n.colorSpace=t.colorSpace),t.flipY!==void 0&&(n.flipY=t.flipY),t.generateMipmaps!==void 0&&(n.generateMipmaps=t.generateMipmaps),t.internalFormat!==void 0&&(n.internalFormat=t.internalFormat);for(let a=0;a<this.textures.length;a++)this.textures[a].setValues(n)}get texture(){return this.textures[0]}set texture(t){this.textures[0]=t}set depthTexture(t){this._depthTexture!==null&&(this._depthTexture.renderTarget=null),t!==null&&(t.renderTarget=this),this._depthTexture=t}get depthTexture(){return this._depthTexture}setSize(t,n,a=1){if(this.width!==t||this.height!==n||this.depth!==a){this.width=t,this.height=n,this.depth=a;for(let l=0,c=this.textures.length;l<c;l++)this.textures[l].image.width=t,this.textures[l].image.height=n,this.textures[l].image.depth=a,this.textures[l].isData3DTexture!==!0&&(this.textures[l].isArrayTexture=this.textures[l].image.depth>1);this.dispose()}this.viewport.set(0,0,t,n),this.scissor.set(0,0,t,n)}clone(){return new this.constructor().copy(this)}copy(t){this.width=t.width,this.height=t.height,this.depth=t.depth,this.scissor.copy(t.scissor),this.scissorTest=t.scissorTest,this.viewport.copy(t.viewport),this.textures.length=0;for(let n=0,a=t.textures.length;n<a;n++){this.textures[n]=t.textures[n].clone(),this.textures[n].isRenderTargetTexture=!0,this.textures[n].renderTarget=this;const l=Object.assign({},t.textures[n].image);this.textures[n].source=new Wp(l)}return this.depthBuffer=t.depthBuffer,this.stencilBuffer=t.stencilBuffer,this.resolveDepthBuffer=t.resolveDepthBuffer,this.resolveStencilBuffer=t.resolveStencilBuffer,t.depthTexture!==null&&(this.depthTexture=t.depthTexture.clone()),this.samples=t.samples,this.multiview=t.multiview,this}dispose(){this.dispatchEvent({type:"dispose"})}}class ta extends Ob{constructor(t=1,n=1,a={}){super(t,n,a),this.isWebGLRenderTarget=!0}}class Ox extends Vn{constructor(t=null,n=1,a=1,l=1){super(null),this.isDataArrayTexture=!0,this.image={data:t,width:n,height:a,depth:l},this.magFilter=Pn,this.minFilter=Pn,this.wrapR=wa,this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1,this.layerUpdates=new Set}addLayerUpdate(t){this.layerUpdates.add(t)}clearLayerUpdates(){this.layerUpdates.clear()}}class Pb extends Vn{constructor(t=null,n=1,a=1,l=1){super(null),this.isData3DTexture=!0,this.image={data:t,width:n,height:a,depth:l},this.magFilter=Pn,this.minFilter=Pn,this.wrapR=wa,this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1}}class tn{static{tn.prototype.isMatrix4=!0}constructor(t,n,a,l,c,f,d,m,h,g,_,v,y,E,A,S){this.elements=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],t!==void 0&&this.set(t,n,a,l,c,f,d,m,h,g,_,v,y,E,A,S)}set(t,n,a,l,c,f,d,m,h,g,_,v,y,E,A,S){const x=this.elements;return x[0]=t,x[4]=n,x[8]=a,x[12]=l,x[1]=c,x[5]=f,x[9]=d,x[13]=m,x[2]=h,x[6]=g,x[10]=_,x[14]=v,x[3]=y,x[7]=E,x[11]=A,x[15]=S,this}identity(){return this.set(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1),this}clone(){return new tn().fromArray(this.elements)}copy(t){const n=this.elements,a=t.elements;return n[0]=a[0],n[1]=a[1],n[2]=a[2],n[3]=a[3],n[4]=a[4],n[5]=a[5],n[6]=a[6],n[7]=a[7],n[8]=a[8],n[9]=a[9],n[10]=a[10],n[11]=a[11],n[12]=a[12],n[13]=a[13],n[14]=a[14],n[15]=a[15],this}copyPosition(t){const n=this.elements,a=t.elements;return n[12]=a[12],n[13]=a[13],n[14]=a[14],this}setFromMatrix3(t){const n=t.elements;return this.set(n[0],n[3],n[6],0,n[1],n[4],n[7],0,n[2],n[5],n[8],0,0,0,0,1),this}extractBasis(t,n,a){return this.determinant()===0?(t.set(1,0,0),n.set(0,1,0),a.set(0,0,1),this):(t.setFromMatrixColumn(this,0),n.setFromMatrixColumn(this,1),a.setFromMatrixColumn(this,2),this)}makeBasis(t,n,a){return this.set(t.x,n.x,a.x,0,t.y,n.y,a.y,0,t.z,n.z,a.z,0,0,0,0,1),this}extractRotation(t){if(t.determinant()===0)return this.identity();const n=this.elements,a=t.elements,l=1/Ir.setFromMatrixColumn(t,0).length(),c=1/Ir.setFromMatrixColumn(t,1).length(),f=1/Ir.setFromMatrixColumn(t,2).length();return n[0]=a[0]*l,n[1]=a[1]*l,n[2]=a[2]*l,n[3]=0,n[4]=a[4]*c,n[5]=a[5]*c,n[6]=a[6]*c,n[7]=0,n[8]=a[8]*f,n[9]=a[9]*f,n[10]=a[10]*f,n[11]=0,n[12]=0,n[13]=0,n[14]=0,n[15]=1,this}makeRotationFromEuler(t){const n=this.elements,a=t.x,l=t.y,c=t.z,f=Math.cos(a),d=Math.sin(a),m=Math.cos(l),h=Math.sin(l),g=Math.cos(c),_=Math.sin(c);if(t.order==="XYZ"){const v=f*g,y=f*_,E=d*g,A=d*_;n[0]=m*g,n[4]=-m*_,n[8]=h,n[1]=y+E*h,n[5]=v-A*h,n[9]=-d*m,n[2]=A-v*h,n[6]=E+y*h,n[10]=f*m}else if(t.order==="YXZ"){const v=m*g,y=m*_,E=h*g,A=h*_;n[0]=v+A*d,n[4]=E*d-y,n[8]=f*h,n[1]=f*_,n[5]=f*g,n[9]=-d,n[2]=y*d-E,n[6]=A+v*d,n[10]=f*m}else if(t.order==="ZXY"){const v=m*g,y=m*_,E=h*g,A=h*_;n[0]=v-A*d,n[4]=-f*_,n[8]=E+y*d,n[1]=y+E*d,n[5]=f*g,n[9]=A-v*d,n[2]=-f*h,n[6]=d,n[10]=f*m}else if(t.order==="ZYX"){const v=f*g,y=f*_,E=d*g,A=d*_;n[0]=m*g,n[4]=E*h-y,n[8]=v*h+A,n[1]=m*_,n[5]=A*h+v,n[9]=y*h-E,n[2]=-h,n[6]=d*m,n[10]=f*m}else if(t.order==="YZX"){const v=f*m,y=f*h,E=d*m,A=d*h;n[0]=m*g,n[4]=A-v*_,n[8]=E*_+y,n[1]=_,n[5]=f*g,n[9]=-d*g,n[2]=-h*g,n[6]=y*_+E,n[10]=v-A*_}else if(t.order==="XZY"){const v=f*m,y=f*h,E=d*m,A=d*h;n[0]=m*g,n[4]=-_,n[8]=h*g,n[1]=v*_+A,n[5]=f*g,n[9]=y*_-E,n[2]=E*_-y,n[6]=d*g,n[10]=A*_+v}return n[3]=0,n[7]=0,n[11]=0,n[12]=0,n[13]=0,n[14]=0,n[15]=1,this}makeRotationFromQuaternion(t){return this.compose(Ib,t,zb)}lookAt(t,n,a){const l=this.elements;return fi.subVectors(t,n),fi.lengthSq()===0&&(fi.z=1),fi.normalize(),cs.crossVectors(a,fi),cs.lengthSq()===0&&(Math.abs(a.z)===1?fi.x+=1e-4:fi.z+=1e-4,fi.normalize(),cs.crossVectors(a,fi)),cs.normalize(),Hc.crossVectors(fi,cs),l[0]=cs.x,l[4]=Hc.x,l[8]=fi.x,l[1]=cs.y,l[5]=Hc.y,l[9]=fi.y,l[2]=cs.z,l[6]=Hc.z,l[10]=fi.z,this}multiply(t){return this.multiplyMatrices(this,t)}premultiply(t){return this.multiplyMatrices(t,this)}multiplyMatrices(t,n){const a=t.elements,l=n.elements,c=this.elements,f=a[0],d=a[4],m=a[8],h=a[12],g=a[1],_=a[5],v=a[9],y=a[13],E=a[2],A=a[6],S=a[10],x=a[14],w=a[3],D=a[7],U=a[11],G=a[15],O=l[0],B=l[4],R=l[8],z=l[12],K=l[1],V=l[5],$=l[9],ht=l[13],gt=l[2],q=l[6],P=l[10],F=l[14],ct=l[3],J=l[7],xt=l[11],I=l[15];return c[0]=f*O+d*K+m*gt+h*ct,c[4]=f*B+d*V+m*q+h*J,c[8]=f*R+d*$+m*P+h*xt,c[12]=f*z+d*ht+m*F+h*I,c[1]=g*O+_*K+v*gt+y*ct,c[5]=g*B+_*V+v*q+y*J,c[9]=g*R+_*$+v*P+y*xt,c[13]=g*z+_*ht+v*F+y*I,c[2]=E*O+A*K+S*gt+x*ct,c[6]=E*B+A*V+S*q+x*J,c[10]=E*R+A*$+S*P+x*xt,c[14]=E*z+A*ht+S*F+x*I,c[3]=w*O+D*K+U*gt+G*ct,c[7]=w*B+D*V+U*q+G*J,c[11]=w*R+D*$+U*P+G*xt,c[15]=w*z+D*ht+U*F+G*I,this}multiplyScalar(t){const n=this.elements;return n[0]*=t,n[4]*=t,n[8]*=t,n[12]*=t,n[1]*=t,n[5]*=t,n[9]*=t,n[13]*=t,n[2]*=t,n[6]*=t,n[10]*=t,n[14]*=t,n[3]*=t,n[7]*=t,n[11]*=t,n[15]*=t,this}determinant(){const t=this.elements,n=t[0],a=t[4],l=t[8],c=t[12],f=t[1],d=t[5],m=t[9],h=t[13],g=t[2],_=t[6],v=t[10],y=t[14],E=t[3],A=t[7],S=t[11],x=t[15],w=m*y-h*v,D=d*y-h*_,U=d*v-m*_,G=f*y-h*g,O=f*v-m*g,B=f*_-d*g;return n*(A*w-S*D+x*U)-a*(E*w-S*G+x*O)+l*(E*D-A*G+x*B)-c*(E*U-A*O+S*B)}transpose(){const t=this.elements;let n;return n=t[1],t[1]=t[4],t[4]=n,n=t[2],t[2]=t[8],t[8]=n,n=t[6],t[6]=t[9],t[9]=n,n=t[3],t[3]=t[12],t[12]=n,n=t[7],t[7]=t[13],t[13]=n,n=t[11],t[11]=t[14],t[14]=n,this}setPosition(t,n,a){const l=this.elements;return t.isVector3?(l[12]=t.x,l[13]=t.y,l[14]=t.z):(l[12]=t,l[13]=n,l[14]=a),this}invert(){const t=this.elements,n=t[0],a=t[1],l=t[2],c=t[3],f=t[4],d=t[5],m=t[6],h=t[7],g=t[8],_=t[9],v=t[10],y=t[11],E=t[12],A=t[13],S=t[14],x=t[15],w=n*d-a*f,D=n*m-l*f,U=n*h-c*f,G=a*m-l*d,O=a*h-c*d,B=l*h-c*m,R=g*A-_*E,z=g*S-v*E,K=g*x-y*E,V=_*S-v*A,$=_*x-y*A,ht=v*x-y*S,gt=w*ht-D*$+U*V+G*K-O*z+B*R;if(gt===0)return this.set(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);const q=1/gt;return t[0]=(d*ht-m*$+h*V)*q,t[1]=(l*$-a*ht-c*V)*q,t[2]=(A*B-S*O+x*G)*q,t[3]=(v*O-_*B-y*G)*q,t[4]=(m*K-f*ht-h*z)*q,t[5]=(n*ht-l*K+c*z)*q,t[6]=(S*U-E*B-x*D)*q,t[7]=(g*B-v*U+y*D)*q,t[8]=(f*$-d*K+h*R)*q,t[9]=(a*K-n*$-c*R)*q,t[10]=(E*O-A*U+x*w)*q,t[11]=(_*U-g*O-y*w)*q,t[12]=(d*z-f*V-m*R)*q,t[13]=(n*V-a*z+l*R)*q,t[14]=(A*D-E*G-S*w)*q,t[15]=(g*G-_*D+v*w)*q,this}scale(t){const n=this.elements,a=t.x,l=t.y,c=t.z;return n[0]*=a,n[4]*=l,n[8]*=c,n[1]*=a,n[5]*=l,n[9]*=c,n[2]*=a,n[6]*=l,n[10]*=c,n[3]*=a,n[7]*=l,n[11]*=c,this}getMaxScaleOnAxis(){const t=this.elements,n=t[0]*t[0]+t[1]*t[1]+t[2]*t[2],a=t[4]*t[4]+t[5]*t[5]+t[6]*t[6],l=t[8]*t[8]+t[9]*t[9]+t[10]*t[10];return Math.sqrt(Math.max(n,a,l))}makeTranslation(t,n,a){return t.isVector3?this.set(1,0,0,t.x,0,1,0,t.y,0,0,1,t.z,0,0,0,1):this.set(1,0,0,t,0,1,0,n,0,0,1,a,0,0,0,1),this}makeRotationX(t){const n=Math.cos(t),a=Math.sin(t);return this.set(1,0,0,0,0,n,-a,0,0,a,n,0,0,0,0,1),this}makeRotationY(t){const n=Math.cos(t),a=Math.sin(t);return this.set(n,0,a,0,0,1,0,0,-a,0,n,0,0,0,0,1),this}makeRotationZ(t){const n=Math.cos(t),a=Math.sin(t);return this.set(n,-a,0,0,a,n,0,0,0,0,1,0,0,0,0,1),this}makeRotationAxis(t,n){const a=Math.cos(n),l=Math.sin(n),c=1-a,f=t.x,d=t.y,m=t.z,h=c*f,g=c*d;return this.set(h*f+a,h*d-l*m,h*m+l*d,0,h*d+l*m,g*d+a,g*m-l*f,0,h*m-l*d,g*m+l*f,c*m*m+a,0,0,0,0,1),this}makeScale(t,n,a){return this.set(t,0,0,0,0,n,0,0,0,0,a,0,0,0,0,1),this}makeShear(t,n,a,l,c,f){return this.set(1,a,c,0,t,1,f,0,n,l,1,0,0,0,0,1),this}compose(t,n,a){const l=this.elements,c=n._x,f=n._y,d=n._z,m=n._w,h=c+c,g=f+f,_=d+d,v=c*h,y=c*g,E=c*_,A=f*g,S=f*_,x=d*_,w=m*h,D=m*g,U=m*_,G=a.x,O=a.y,B=a.z;return l[0]=(1-(A+x))*G,l[1]=(y+U)*G,l[2]=(E-D)*G,l[3]=0,l[4]=(y-U)*O,l[5]=(1-(v+x))*O,l[6]=(S+w)*O,l[7]=0,l[8]=(E+D)*B,l[9]=(S-w)*B,l[10]=(1-(v+A))*B,l[11]=0,l[12]=t.x,l[13]=t.y,l[14]=t.z,l[15]=1,this}decompose(t,n,a){const l=this.elements;t.x=l[12],t.y=l[13],t.z=l[14];const c=this.determinant();if(c===0)return a.set(1,1,1),n.identity(),this;let f=Ir.set(l[0],l[1],l[2]).length();const d=Ir.set(l[4],l[5],l[6]).length(),m=Ir.set(l[8],l[9],l[10]).length();c<0&&(f=-f),Ii.copy(this);const h=1/f,g=1/d,_=1/m;return Ii.elements[0]*=h,Ii.elements[1]*=h,Ii.elements[2]*=h,Ii.elements[4]*=g,Ii.elements[5]*=g,Ii.elements[6]*=g,Ii.elements[8]*=_,Ii.elements[9]*=_,Ii.elements[10]*=_,n.setFromRotationMatrix(Ii),a.x=f,a.y=d,a.z=m,this}makePerspective(t,n,a,l,c,f,d=Ji,m=!1){const h=this.elements,g=2*c/(n-t),_=2*c/(a-l),v=(n+t)/(n-t),y=(a+l)/(a-l);let E,A;if(m)E=c/(f-c),A=f*c/(f-c);else if(d===Ji)E=-(f+c)/(f-c),A=-2*f*c/(f-c);else if(d===El)E=-f/(f-c),A=-f*c/(f-c);else throw new Error("THREE.Matrix4.makePerspective(): Invalid coordinate system: "+d);return h[0]=g,h[4]=0,h[8]=v,h[12]=0,h[1]=0,h[5]=_,h[9]=y,h[13]=0,h[2]=0,h[6]=0,h[10]=E,h[14]=A,h[3]=0,h[7]=0,h[11]=-1,h[15]=0,this}makeOrthographic(t,n,a,l,c,f,d=Ji,m=!1){const h=this.elements,g=2/(n-t),_=2/(a-l),v=-(n+t)/(n-t),y=-(a+l)/(a-l);let E,A;if(m)E=1/(f-c),A=f/(f-c);else if(d===Ji)E=-2/(f-c),A=-(f+c)/(f-c);else if(d===El)E=-1/(f-c),A=-c/(f-c);else throw new Error("THREE.Matrix4.makeOrthographic(): Invalid coordinate system: "+d);return h[0]=g,h[4]=0,h[8]=0,h[12]=v,h[1]=0,h[5]=_,h[9]=0,h[13]=y,h[2]=0,h[6]=0,h[10]=E,h[14]=A,h[3]=0,h[7]=0,h[11]=0,h[15]=1,this}equals(t){const n=this.elements,a=t.elements;for(let l=0;l<16;l++)if(n[l]!==a[l])return!1;return!0}fromArray(t,n=0){for(let a=0;a<16;a++)this.elements[a]=t[a+n];return this}toArray(t=[],n=0){const a=this.elements;return t[n]=a[0],t[n+1]=a[1],t[n+2]=a[2],t[n+3]=a[3],t[n+4]=a[4],t[n+5]=a[5],t[n+6]=a[6],t[n+7]=a[7],t[n+8]=a[8],t[n+9]=a[9],t[n+10]=a[10],t[n+11]=a[11],t[n+12]=a[12],t[n+13]=a[13],t[n+14]=a[14],t[n+15]=a[15],t}}const Ir=new k,Ii=new tn,Ib=new k(0,0,0),zb=new k(1,1,1),cs=new k,Hc=new k,fi=new k,sv=new tn,rv=new wi;class xs{constructor(t=0,n=0,a=0,l=xs.DEFAULT_ORDER){this.isEuler=!0,this._x=t,this._y=n,this._z=a,this._order=l}get x(){return this._x}set x(t){this._x=t,this._onChangeCallback()}get y(){return this._y}set y(t){this._y=t,this._onChangeCallback()}get z(){return this._z}set z(t){this._z=t,this._onChangeCallback()}get order(){return this._order}set order(t){this._order=t,this._onChangeCallback()}set(t,n,a,l=this._order){return this._x=t,this._y=n,this._z=a,this._order=l,this._onChangeCallback(),this}clone(){return new this.constructor(this._x,this._y,this._z,this._order)}copy(t){return this._x=t._x,this._y=t._y,this._z=t._z,this._order=t._order,this._onChangeCallback(),this}setFromRotationMatrix(t,n=this._order,a=!0){const l=t.elements,c=l[0],f=l[4],d=l[8],m=l[1],h=l[5],g=l[9],_=l[2],v=l[6],y=l[10];switch(n){case"XYZ":this._y=Math.asin(Se(d,-1,1)),Math.abs(d)<.9999999?(this._x=Math.atan2(-g,y),this._z=Math.atan2(-f,c)):(this._x=Math.atan2(v,h),this._z=0);break;case"YXZ":this._x=Math.asin(-Se(g,-1,1)),Math.abs(g)<.9999999?(this._y=Math.atan2(d,y),this._z=Math.atan2(m,h)):(this._y=Math.atan2(-_,c),this._z=0);break;case"ZXY":this._x=Math.asin(Se(v,-1,1)),Math.abs(v)<.9999999?(this._y=Math.atan2(-_,y),this._z=Math.atan2(-f,h)):(this._y=0,this._z=Math.atan2(m,c));break;case"ZYX":this._y=Math.asin(-Se(_,-1,1)),Math.abs(_)<.9999999?(this._x=Math.atan2(v,y),this._z=Math.atan2(m,c)):(this._x=0,this._z=Math.atan2(-f,h));break;case"YZX":this._z=Math.asin(Se(m,-1,1)),Math.abs(m)<.9999999?(this._x=Math.atan2(-g,h),this._y=Math.atan2(-_,c)):(this._x=0,this._y=Math.atan2(d,y));break;case"XZY":this._z=Math.asin(-Se(f,-1,1)),Math.abs(f)<.9999999?(this._x=Math.atan2(v,h),this._y=Math.atan2(d,c)):(this._x=Math.atan2(-g,y),this._y=0);break;default:ce("Euler: .setFromRotationMatrix() encountered an unknown order: "+n)}return this._order=n,a===!0&&this._onChangeCallback(),this}setFromQuaternion(t,n,a){return sv.makeRotationFromQuaternion(t),this.setFromRotationMatrix(sv,n,a)}setFromVector3(t,n=this._order){return this.set(t.x,t.y,t.z,n)}reorder(t){return rv.setFromEuler(this),this.setFromQuaternion(rv,t)}equals(t){return t._x===this._x&&t._y===this._y&&t._z===this._z&&t._order===this._order}fromArray(t){return this._x=t[0],this._y=t[1],this._z=t[2],t[3]!==void 0&&(this._order=t[3]),this._onChangeCallback(),this}toArray(t=[],n=0){return t[n]=this._x,t[n+1]=this._y,t[n+2]=this._z,t[n+3]=this._order,t}_onChange(t){return this._onChangeCallback=t,this}_onChangeCallback(){}*[Symbol.iterator](){yield this._x,yield this._y,yield this._z,yield this._order}}xs.DEFAULT_ORDER="XYZ";class qp{constructor(){this.mask=1}set(t){this.mask=(1<<t|0)>>>0}enable(t){this.mask|=1<<t|0}enableAll(){this.mask=-1}toggle(t){this.mask^=1<<t|0}disable(t){this.mask&=~(1<<t|0)}disableAll(){this.mask=0}test(t){return(this.mask&t.mask)!==0}isEnabled(t){return(this.mask&(1<<t|0))!==0}}let Bb=0;const ov=new k,zr=new wi,Sa=new tn,Gc=new k,sl=new k,Fb=new k,Hb=new wi,lv=new k(1,0,0),cv=new k(0,1,0),uv=new k(0,0,1),fv={type:"added"},Gb={type:"removed"},Br={type:"childadded",child:null},ah={type:"childremoved",child:null};class kn extends tr{constructor(){super(),this.isObject3D=!0,Object.defineProperty(this,"id",{value:Bb++}),this.uuid=Ca(),this.name="",this.type="Object3D",this.parent=null,this.children=[],this.up=kn.DEFAULT_UP.clone();const t=new k,n=new xs,a=new wi,l=new k(1,1,1);function c(){a.setFromEuler(n,!1)}function f(){n.setFromQuaternion(a,void 0,!1)}n._onChange(c),a._onChange(f),Object.defineProperties(this,{position:{configurable:!0,enumerable:!0,value:t},rotation:{configurable:!0,enumerable:!0,value:n},quaternion:{configurable:!0,enumerable:!0,value:a},scale:{configurable:!0,enumerable:!0,value:l},modelViewMatrix:{value:new tn},normalMatrix:{value:new pe}}),this.matrix=new tn,this.matrixWorld=new tn,this.matrixAutoUpdate=kn.DEFAULT_MATRIX_AUTO_UPDATE,this.matrixWorldAutoUpdate=kn.DEFAULT_MATRIX_WORLD_AUTO_UPDATE,this.matrixWorldNeedsUpdate=!1,this.layers=new qp,this.visible=!0,this.castShadow=!1,this.receiveShadow=!1,this.frustumCulled=!0,this.renderOrder=0,this.animations=[],this.customDepthMaterial=void 0,this.customDistanceMaterial=void 0,this.static=!1,this.userData={},this.pivot=null}onBeforeShadow(){}onAfterShadow(){}onBeforeRender(){}onAfterRender(){}applyMatrix4(t){this.matrixAutoUpdate&&this.updateMatrix(),this.matrix.premultiply(t),this.matrix.decompose(this.position,this.quaternion,this.scale)}applyQuaternion(t){return this.quaternion.premultiply(t),this}setRotationFromAxisAngle(t,n){this.quaternion.setFromAxisAngle(t,n)}setRotationFromEuler(t){this.quaternion.setFromEuler(t,!0)}setRotationFromMatrix(t){this.quaternion.setFromRotationMatrix(t)}setRotationFromQuaternion(t){this.quaternion.copy(t)}rotateOnAxis(t,n){return zr.setFromAxisAngle(t,n),this.quaternion.multiply(zr),this}rotateOnWorldAxis(t,n){return zr.setFromAxisAngle(t,n),this.quaternion.premultiply(zr),this}rotateX(t){return this.rotateOnAxis(lv,t)}rotateY(t){return this.rotateOnAxis(cv,t)}rotateZ(t){return this.rotateOnAxis(uv,t)}translateOnAxis(t,n){return ov.copy(t).applyQuaternion(this.quaternion),this.position.add(ov.multiplyScalar(n)),this}translateX(t){return this.translateOnAxis(lv,t)}translateY(t){return this.translateOnAxis(cv,t)}translateZ(t){return this.translateOnAxis(uv,t)}localToWorld(t){return this.updateWorldMatrix(!0,!1),t.applyMatrix4(this.matrixWorld)}worldToLocal(t){return this.updateWorldMatrix(!0,!1),t.applyMatrix4(Sa.copy(this.matrixWorld).invert())}lookAt(t,n,a){t.isVector3?Gc.copy(t):Gc.set(t,n,a);const l=this.parent;this.updateWorldMatrix(!0,!1),sl.setFromMatrixPosition(this.matrixWorld),this.isCamera||this.isLight?Sa.lookAt(sl,Gc,this.up):Sa.lookAt(Gc,sl,this.up),this.quaternion.setFromRotationMatrix(Sa),l&&(Sa.extractRotation(l.matrixWorld),zr.setFromRotationMatrix(Sa),this.quaternion.premultiply(zr.invert()))}add(t){if(arguments.length>1){for(let n=0;n<arguments.length;n++)this.add(arguments[n]);return this}return t===this?(Ne("Object3D.add: object can't be added as a child of itself.",t),this):(t&&t.isObject3D?(t.removeFromParent(),t.parent=this,this.children.push(t),t.dispatchEvent(fv),Br.child=t,this.dispatchEvent(Br),Br.child=null):Ne("Object3D.add: object not an instance of THREE.Object3D.",t),this)}remove(t){if(arguments.length>1){for(let a=0;a<arguments.length;a++)this.remove(arguments[a]);return this}const n=this.children.indexOf(t);return n!==-1&&(t.parent=null,this.children.splice(n,1),t.dispatchEvent(Gb),ah.child=t,this.dispatchEvent(ah),ah.child=null),this}removeFromParent(){const t=this.parent;return t!==null&&t.remove(this),this}clear(){return this.remove(...this.children)}attach(t){return this.updateWorldMatrix(!0,!1),Sa.copy(this.matrixWorld).invert(),t.parent!==null&&(t.parent.updateWorldMatrix(!0,!1),Sa.multiply(t.parent.matrixWorld)),t.applyMatrix4(Sa),t.removeFromParent(),t.parent=this,this.children.push(t),t.updateWorldMatrix(!1,!0),t.dispatchEvent(fv),Br.child=t,this.dispatchEvent(Br),Br.child=null,this}getObjectById(t){return this.getObjectByProperty("id",t)}getObjectByName(t){return this.getObjectByProperty("name",t)}getObjectByProperty(t,n){if(this[t]===n)return this;for(let a=0,l=this.children.length;a<l;a++){const f=this.children[a].getObjectByProperty(t,n);if(f!==void 0)return f}}getObjectsByProperty(t,n,a=[]){this[t]===n&&a.push(this);const l=this.children;for(let c=0,f=l.length;c<f;c++)l[c].getObjectsByProperty(t,n,a);return a}getWorldPosition(t){return this.updateWorldMatrix(!0,!1),t.setFromMatrixPosition(this.matrixWorld)}getWorldQuaternion(t){return this.updateWorldMatrix(!0,!1),this.matrixWorld.decompose(sl,t,Fb),t}getWorldScale(t){return this.updateWorldMatrix(!0,!1),this.matrixWorld.decompose(sl,Hb,t),t}getWorldDirection(t){this.updateWorldMatrix(!0,!1);const n=this.matrixWorld.elements;return t.set(n[8],n[9],n[10]).normalize()}raycast(){}traverse(t){t(this);const n=this.children;for(let a=0,l=n.length;a<l;a++)n[a].traverse(t)}traverseVisible(t){if(this.visible===!1)return;t(this);const n=this.children;for(let a=0,l=n.length;a<l;a++)n[a].traverseVisible(t)}traverseAncestors(t){const n=this.parent;n!==null&&(t(n),n.traverseAncestors(t))}updateMatrix(){this.matrix.compose(this.position,this.quaternion,this.scale);const t=this.pivot;if(t!==null){const n=t.x,a=t.y,l=t.z,c=this.matrix.elements;c[12]+=n-c[0]*n-c[4]*a-c[8]*l,c[13]+=a-c[1]*n-c[5]*a-c[9]*l,c[14]+=l-c[2]*n-c[6]*a-c[10]*l}this.matrixWorldNeedsUpdate=!0}updateMatrixWorld(t){this.matrixAutoUpdate&&this.updateMatrix(),(this.matrixWorldNeedsUpdate||t)&&(this.matrixWorldAutoUpdate===!0&&(this.parent===null?this.matrixWorld.copy(this.matrix):this.matrixWorld.multiplyMatrices(this.parent.matrixWorld,this.matrix)),this.matrixWorldNeedsUpdate=!1,t=!0);const n=this.children;for(let a=0,l=n.length;a<l;a++)n[a].updateMatrixWorld(t)}updateWorldMatrix(t,n){const a=this.parent;if(t===!0&&a!==null&&a.updateWorldMatrix(!0,!1),this.matrixAutoUpdate&&this.updateMatrix(),this.matrixWorldAutoUpdate===!0&&(this.parent===null?this.matrixWorld.copy(this.matrix):this.matrixWorld.multiplyMatrices(this.parent.matrixWorld,this.matrix)),n===!0){const l=this.children;for(let c=0,f=l.length;c<f;c++)l[c].updateWorldMatrix(!1,!0)}}toJSON(t){const n=t===void 0||typeof t=="string",a={};n&&(t={geometries:{},materials:{},textures:{},images:{},shapes:{},skeletons:{},animations:{},nodes:{}},a.metadata={version:4.7,type:"Object",generator:"Object3D.toJSON"});const l={};l.uuid=this.uuid,l.type=this.type,this.name!==""&&(l.name=this.name),this.castShadow===!0&&(l.castShadow=!0),this.receiveShadow===!0&&(l.receiveShadow=!0),this.visible===!1&&(l.visible=!1),this.frustumCulled===!1&&(l.frustumCulled=!1),this.renderOrder!==0&&(l.renderOrder=this.renderOrder),this.static!==!1&&(l.static=this.static),Object.keys(this.userData).length>0&&(l.userData=this.userData),l.layers=this.layers.mask,l.matrix=this.matrix.toArray(),l.up=this.up.toArray(),this.pivot!==null&&(l.pivot=this.pivot.toArray()),this.matrixAutoUpdate===!1&&(l.matrixAutoUpdate=!1),this.morphTargetDictionary!==void 0&&(l.morphTargetDictionary=Object.assign({},this.morphTargetDictionary)),this.morphTargetInfluences!==void 0&&(l.morphTargetInfluences=this.morphTargetInfluences.slice()),this.isInstancedMesh&&(l.type="InstancedMesh",l.count=this.count,l.instanceMatrix=this.instanceMatrix.toJSON(),this.instanceColor!==null&&(l.instanceColor=this.instanceColor.toJSON())),this.isBatchedMesh&&(l.type="BatchedMesh",l.perObjectFrustumCulled=this.perObjectFrustumCulled,l.sortObjects=this.sortObjects,l.drawRanges=this._drawRanges,l.reservedRanges=this._reservedRanges,l.geometryInfo=this._geometryInfo.map(d=>({...d,boundingBox:d.boundingBox?d.boundingBox.toJSON():void 0,boundingSphere:d.boundingSphere?d.boundingSphere.toJSON():void 0})),l.instanceInfo=this._instanceInfo.map(d=>({...d})),l.availableInstanceIds=this._availableInstanceIds.slice(),l.availableGeometryIds=this._availableGeometryIds.slice(),l.nextIndexStart=this._nextIndexStart,l.nextVertexStart=this._nextVertexStart,l.geometryCount=this._geometryCount,l.maxInstanceCount=this._maxInstanceCount,l.maxVertexCount=this._maxVertexCount,l.maxIndexCount=this._maxIndexCount,l.geometryInitialized=this._geometryInitialized,l.matricesTexture=this._matricesTexture.toJSON(t),l.indirectTexture=this._indirectTexture.toJSON(t),this._colorsTexture!==null&&(l.colorsTexture=this._colorsTexture.toJSON(t)),this.boundingSphere!==null&&(l.boundingSphere=this.boundingSphere.toJSON()),this.boundingBox!==null&&(l.boundingBox=this.boundingBox.toJSON()));function c(d,m){return d[m.uuid]===void 0&&(d[m.uuid]=m.toJSON(t)),m.uuid}if(this.isScene)this.background&&(this.background.isColor?l.background=this.background.toJSON():this.background.isTexture&&(l.background=this.background.toJSON(t).uuid)),this.environment&&this.environment.isTexture&&this.environment.isRenderTargetTexture!==!0&&(l.environment=this.environment.toJSON(t).uuid);else if(this.isMesh||this.isLine||this.isPoints){l.geometry=c(t.geometries,this.geometry);const d=this.geometry.parameters;if(d!==void 0&&d.shapes!==void 0){const m=d.shapes;if(Array.isArray(m))for(let h=0,g=m.length;h<g;h++){const _=m[h];c(t.shapes,_)}else c(t.shapes,m)}}if(this.isSkinnedMesh&&(l.bindMode=this.bindMode,l.bindMatrix=this.bindMatrix.toArray(),this.skeleton!==void 0&&(c(t.skeletons,this.skeleton),l.skeleton=this.skeleton.uuid)),this.material!==void 0)if(Array.isArray(this.material)){const d=[];for(let m=0,h=this.material.length;m<h;m++)d.push(c(t.materials,this.material[m]));l.material=d}else l.material=c(t.materials,this.material);if(this.children.length>0){l.children=[];for(let d=0;d<this.children.length;d++)l.children.push(this.children[d].toJSON(t).object)}if(this.animations.length>0){l.animations=[];for(let d=0;d<this.animations.length;d++){const m=this.animations[d];l.animations.push(c(t.animations,m))}}if(n){const d=f(t.geometries),m=f(t.materials),h=f(t.textures),g=f(t.images),_=f(t.shapes),v=f(t.skeletons),y=f(t.animations),E=f(t.nodes);d.length>0&&(a.geometries=d),m.length>0&&(a.materials=m),h.length>0&&(a.textures=h),g.length>0&&(a.images=g),_.length>0&&(a.shapes=_),v.length>0&&(a.skeletons=v),y.length>0&&(a.animations=y),E.length>0&&(a.nodes=E)}return a.object=l,a;function f(d){const m=[];for(const h in d){const g=d[h];delete g.metadata,m.push(g)}return m}}clone(t){return new this.constructor().copy(this,t)}copy(t,n=!0){if(this.name=t.name,this.up.copy(t.up),this.position.copy(t.position),this.rotation.order=t.rotation.order,this.quaternion.copy(t.quaternion),this.scale.copy(t.scale),this.pivot=t.pivot!==null?t.pivot.clone():null,this.matrix.copy(t.matrix),this.matrixWorld.copy(t.matrixWorld),this.matrixAutoUpdate=t.matrixAutoUpdate,this.matrixWorldAutoUpdate=t.matrixWorldAutoUpdate,this.matrixWorldNeedsUpdate=t.matrixWorldNeedsUpdate,this.layers.mask=t.layers.mask,this.visible=t.visible,this.castShadow=t.castShadow,this.receiveShadow=t.receiveShadow,this.frustumCulled=t.frustumCulled,this.renderOrder=t.renderOrder,this.static=t.static,this.animations=t.animations.slice(),this.userData=JSON.parse(JSON.stringify(t.userData)),n===!0)for(let a=0;a<t.children.length;a++){const l=t.children[a];this.add(l.clone())}return this}}kn.DEFAULT_UP=new k(0,1,0);kn.DEFAULT_MATRIX_AUTO_UPDATE=!0;kn.DEFAULT_MATRIX_WORLD_AUTO_UPDATE=!0;class Qs extends kn{constructor(){super(),this.isGroup=!0,this.type="Group"}}const Vb={type:"move"};class sh{constructor(){this._targetRay=null,this._grip=null,this._hand=null}getHandSpace(){return this._hand===null&&(this._hand=new Qs,this._hand.matrixAutoUpdate=!1,this._hand.visible=!1,this._hand.joints={},this._hand.inputState={pinching:!1}),this._hand}getTargetRaySpace(){return this._targetRay===null&&(this._targetRay=new Qs,this._targetRay.matrixAutoUpdate=!1,this._targetRay.visible=!1,this._targetRay.hasLinearVelocity=!1,this._targetRay.linearVelocity=new k,this._targetRay.hasAngularVelocity=!1,this._targetRay.angularVelocity=new k),this._targetRay}getGripSpace(){return this._grip===null&&(this._grip=new Qs,this._grip.matrixAutoUpdate=!1,this._grip.visible=!1,this._grip.hasLinearVelocity=!1,this._grip.linearVelocity=new k,this._grip.hasAngularVelocity=!1,this._grip.angularVelocity=new k,this._grip.eventsEnabled=!1),this._grip}dispatchEvent(t){return this._targetRay!==null&&this._targetRay.dispatchEvent(t),this._grip!==null&&this._grip.dispatchEvent(t),this._hand!==null&&this._hand.dispatchEvent(t),this}connect(t){if(t&&t.hand){const n=this._hand;if(n)for(const a of t.hand.values())this._getHandJoint(n,a)}return this.dispatchEvent({type:"connected",data:t}),this}disconnect(t){return this.dispatchEvent({type:"disconnected",data:t}),this._targetRay!==null&&(this._targetRay.visible=!1),this._grip!==null&&(this._grip.visible=!1),this._hand!==null&&(this._hand.visible=!1),this}update(t,n,a){let l=null,c=null,f=null;const d=this._targetRay,m=this._grip,h=this._hand;if(t&&n.session.visibilityState!=="visible-blurred"){if(h&&t.hand){f=!0;for(const A of t.hand.values()){const S=n.getJointPose(A,a),x=this._getHandJoint(h,A);S!==null&&(x.matrix.fromArray(S.transform.matrix),x.matrix.decompose(x.position,x.rotation,x.scale),x.matrixWorldNeedsUpdate=!0,x.jointRadius=S.radius),x.visible=S!==null}const g=h.joints["index-finger-tip"],_=h.joints["thumb-tip"],v=g.position.distanceTo(_.position),y=.02,E=.005;h.inputState.pinching&&v>y+E?(h.inputState.pinching=!1,this.dispatchEvent({type:"pinchend",handedness:t.handedness,target:this})):!h.inputState.pinching&&v<=y-E&&(h.inputState.pinching=!0,this.dispatchEvent({type:"pinchstart",handedness:t.handedness,target:this}))}else m!==null&&t.gripSpace&&(c=n.getPose(t.gripSpace,a),c!==null&&(m.matrix.fromArray(c.transform.matrix),m.matrix.decompose(m.position,m.rotation,m.scale),m.matrixWorldNeedsUpdate=!0,c.linearVelocity?(m.hasLinearVelocity=!0,m.linearVelocity.copy(c.linearVelocity)):m.hasLinearVelocity=!1,c.angularVelocity?(m.hasAngularVelocity=!0,m.angularVelocity.copy(c.angularVelocity)):m.hasAngularVelocity=!1,m.eventsEnabled&&m.dispatchEvent({type:"gripUpdated",data:t,target:this})));d!==null&&(l=n.getPose(t.targetRaySpace,a),l===null&&c!==null&&(l=c),l!==null&&(d.matrix.fromArray(l.transform.matrix),d.matrix.decompose(d.position,d.rotation,d.scale),d.matrixWorldNeedsUpdate=!0,l.linearVelocity?(d.hasLinearVelocity=!0,d.linearVelocity.copy(l.linearVelocity)):d.hasLinearVelocity=!1,l.angularVelocity?(d.hasAngularVelocity=!0,d.angularVelocity.copy(l.angularVelocity)):d.hasAngularVelocity=!1,this.dispatchEvent(Vb)))}return d!==null&&(d.visible=l!==null),m!==null&&(m.visible=c!==null),h!==null&&(h.visible=f!==null),this}_getHandJoint(t,n){if(t.joints[n.jointName]===void 0){const a=new Qs;a.matrixAutoUpdate=!1,a.visible=!1,t.joints[n.jointName]=a,t.add(a)}return t.joints[n.jointName]}}const Px={aliceblue:15792383,antiquewhite:16444375,aqua:65535,aquamarine:8388564,azure:15794175,beige:16119260,bisque:16770244,black:0,blanchedalmond:16772045,blue:255,blueviolet:9055202,brown:10824234,burlywood:14596231,cadetblue:6266528,chartreuse:8388352,chocolate:13789470,coral:16744272,cornflowerblue:6591981,cornsilk:16775388,crimson:14423100,cyan:65535,darkblue:139,darkcyan:35723,darkgoldenrod:12092939,darkgray:11119017,darkgreen:25600,darkgrey:11119017,darkkhaki:12433259,darkmagenta:9109643,darkolivegreen:5597999,darkorange:16747520,darkorchid:10040012,darkred:9109504,darksalmon:15308410,darkseagreen:9419919,darkslateblue:4734347,darkslategray:3100495,darkslategrey:3100495,darkturquoise:52945,darkviolet:9699539,deeppink:16716947,deepskyblue:49151,dimgray:6908265,dimgrey:6908265,dodgerblue:2003199,firebrick:11674146,floralwhite:16775920,forestgreen:2263842,fuchsia:16711935,gainsboro:14474460,ghostwhite:16316671,gold:16766720,goldenrod:14329120,gray:8421504,green:32768,greenyellow:11403055,grey:8421504,honeydew:15794160,hotpink:16738740,indianred:13458524,indigo:4915330,ivory:16777200,khaki:15787660,lavender:15132410,lavenderblush:16773365,lawngreen:8190976,lemonchiffon:16775885,lightblue:11393254,lightcoral:15761536,lightcyan:14745599,lightgoldenrodyellow:16448210,lightgray:13882323,lightgreen:9498256,lightgrey:13882323,lightpink:16758465,lightsalmon:16752762,lightseagreen:2142890,lightskyblue:8900346,lightslategray:7833753,lightslategrey:7833753,lightsteelblue:11584734,lightyellow:16777184,lime:65280,limegreen:3329330,linen:16445670,magenta:16711935,maroon:8388608,mediumaquamarine:6737322,mediumblue:205,mediumorchid:12211667,mediumpurple:9662683,mediumseagreen:3978097,mediumslateblue:8087790,mediumspringgreen:64154,mediumturquoise:4772300,mediumvioletred:13047173,midnightblue:1644912,mintcream:16121850,mistyrose:16770273,moccasin:16770229,navajowhite:16768685,navy:128,oldlace:16643558,olive:8421376,olivedrab:7048739,orange:16753920,orangered:16729344,orchid:14315734,palegoldenrod:15657130,palegreen:10025880,paleturquoise:11529966,palevioletred:14381203,papayawhip:16773077,peachpuff:16767673,peru:13468991,pink:16761035,plum:14524637,powderblue:11591910,purple:8388736,rebeccapurple:6697881,red:16711680,rosybrown:12357519,royalblue:4286945,saddlebrown:9127187,salmon:16416882,sandybrown:16032864,seagreen:3050327,seashell:16774638,sienna:10506797,silver:12632256,skyblue:8900331,slateblue:6970061,slategray:7372944,slategrey:7372944,snow:16775930,springgreen:65407,steelblue:4620980,tan:13808780,teal:32896,thistle:14204888,tomato:16737095,turquoise:4251856,violet:15631086,wheat:16113331,white:16777215,whitesmoke:16119285,yellow:16776960,yellowgreen:10145074},us={h:0,s:0,l:0},Vc={h:0,s:0,l:0};function rh(r,t,n){return n<0&&(n+=1),n>1&&(n-=1),n<1/6?r+(t-r)*6*n:n<1/2?t:n<2/3?r+(t-r)*6*(2/3-n):r}class _e{constructor(t,n,a){return this.isColor=!0,this.r=1,this.g=1,this.b=1,this.set(t,n,a)}set(t,n,a){if(n===void 0&&a===void 0){const l=t;l&&l.isColor?this.copy(l):typeof l=="number"?this.setHex(l):typeof l=="string"&&this.setStyle(l)}else this.setRGB(t,n,a);return this}setScalar(t){return this.r=t,this.g=t,this.b=t,this}setHex(t,n=Ti){return t=Math.floor(t),this.r=(t>>16&255)/255,this.g=(t>>8&255)/255,this.b=(t&255)/255,De.colorSpaceToWorking(this,n),this}setRGB(t,n,a,l=De.workingColorSpace){return this.r=t,this.g=n,this.b=a,De.colorSpaceToWorking(this,l),this}setHSL(t,n,a,l=De.workingColorSpace){if(t=Xp(t,1),n=Se(n,0,1),a=Se(a,0,1),n===0)this.r=this.g=this.b=a;else{const c=a<=.5?a*(1+n):a+n-a*n,f=2*a-c;this.r=rh(f,c,t+1/3),this.g=rh(f,c,t),this.b=rh(f,c,t-1/3)}return De.colorSpaceToWorking(this,l),this}setStyle(t,n=Ti){function a(c){c!==void 0&&parseFloat(c)<1&&ce("Color: Alpha component of "+t+" will be ignored.")}let l;if(l=/^(\w+)\(([^\)]*)\)/.exec(t)){let c;const f=l[1],d=l[2];switch(f){case"rgb":case"rgba":if(c=/^\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(d))return a(c[4]),this.setRGB(Math.min(255,parseInt(c[1],10))/255,Math.min(255,parseInt(c[2],10))/255,Math.min(255,parseInt(c[3],10))/255,n);if(c=/^\s*(\d+)\%\s*,\s*(\d+)\%\s*,\s*(\d+)\%\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(d))return a(c[4]),this.setRGB(Math.min(100,parseInt(c[1],10))/100,Math.min(100,parseInt(c[2],10))/100,Math.min(100,parseInt(c[3],10))/100,n);break;case"hsl":case"hsla":if(c=/^\s*(\d*\.?\d+)\s*,\s*(\d*\.?\d+)\%\s*,\s*(\d*\.?\d+)\%\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(d))return a(c[4]),this.setHSL(parseFloat(c[1])/360,parseFloat(c[2])/100,parseFloat(c[3])/100,n);break;default:ce("Color: Unknown color model "+t)}}else if(l=/^\#([A-Fa-f\d]+)$/.exec(t)){const c=l[1],f=c.length;if(f===3)return this.setRGB(parseInt(c.charAt(0),16)/15,parseInt(c.charAt(1),16)/15,parseInt(c.charAt(2),16)/15,n);if(f===6)return this.setHex(parseInt(c,16),n);ce("Color: Invalid hex color "+t)}else if(t&&t.length>0)return this.setColorName(t,n);return this}setColorName(t,n=Ti){const a=Px[t.toLowerCase()];return a!==void 0?this.setHex(a,n):ce("Color: Unknown color "+t),this}clone(){return new this.constructor(this.r,this.g,this.b)}copy(t){return this.r=t.r,this.g=t.g,this.b=t.b,this}copySRGBToLinear(t){return this.r=Na(t.r),this.g=Na(t.g),this.b=Na(t.b),this}copyLinearToSRGB(t){return this.r=ao(t.r),this.g=ao(t.g),this.b=ao(t.b),this}convertSRGBToLinear(){return this.copySRGBToLinear(this),this}convertLinearToSRGB(){return this.copyLinearToSRGB(this),this}getHex(t=Ti){return De.workingToColorSpace(Hn.copy(this),t),Math.round(Se(Hn.r*255,0,255))*65536+Math.round(Se(Hn.g*255,0,255))*256+Math.round(Se(Hn.b*255,0,255))}getHexString(t=Ti){return("000000"+this.getHex(t).toString(16)).slice(-6)}getHSL(t,n=De.workingColorSpace){De.workingToColorSpace(Hn.copy(this),n);const a=Hn.r,l=Hn.g,c=Hn.b,f=Math.max(a,l,c),d=Math.min(a,l,c);let m,h;const g=(d+f)/2;if(d===f)m=0,h=0;else{const _=f-d;switch(h=g<=.5?_/(f+d):_/(2-f-d),f){case a:m=(l-c)/_+(l<c?6:0);break;case l:m=(c-a)/_+2;break;case c:m=(a-l)/_+4;break}m/=6}return t.h=m,t.s=h,t.l=g,t}getRGB(t,n=De.workingColorSpace){return De.workingToColorSpace(Hn.copy(this),n),t.r=Hn.r,t.g=Hn.g,t.b=Hn.b,t}getStyle(t=Ti){De.workingToColorSpace(Hn.copy(this),t);const n=Hn.r,a=Hn.g,l=Hn.b;return t!==Ti?`color(${t} ${n.toFixed(3)} ${a.toFixed(3)} ${l.toFixed(3)})`:`rgb(${Math.round(n*255)},${Math.round(a*255)},${Math.round(l*255)})`}offsetHSL(t,n,a){return this.getHSL(us),this.setHSL(us.h+t,us.s+n,us.l+a)}add(t){return this.r+=t.r,this.g+=t.g,this.b+=t.b,this}addColors(t,n){return this.r=t.r+n.r,this.g=t.g+n.g,this.b=t.b+n.b,this}addScalar(t){return this.r+=t,this.g+=t,this.b+=t,this}sub(t){return this.r=Math.max(0,this.r-t.r),this.g=Math.max(0,this.g-t.g),this.b=Math.max(0,this.b-t.b),this}multiply(t){return this.r*=t.r,this.g*=t.g,this.b*=t.b,this}multiplyScalar(t){return this.r*=t,this.g*=t,this.b*=t,this}lerp(t,n){return this.r+=(t.r-this.r)*n,this.g+=(t.g-this.g)*n,this.b+=(t.b-this.b)*n,this}lerpColors(t,n,a){return this.r=t.r+(n.r-t.r)*a,this.g=t.g+(n.g-t.g)*a,this.b=t.b+(n.b-t.b)*a,this}lerpHSL(t,n){this.getHSL(us),t.getHSL(Vc);const a=xl(us.h,Vc.h,n),l=xl(us.s,Vc.s,n),c=xl(us.l,Vc.l,n);return this.setHSL(a,l,c),this}setFromVector3(t){return this.r=t.x,this.g=t.y,this.b=t.z,this}applyMatrix3(t){const n=this.r,a=this.g,l=this.b,c=t.elements;return this.r=c[0]*n+c[3]*a+c[6]*l,this.g=c[1]*n+c[4]*a+c[7]*l,this.b=c[2]*n+c[5]*a+c[8]*l,this}equals(t){return t.r===this.r&&t.g===this.g&&t.b===this.b}fromArray(t,n=0){return this.r=t[n],this.g=t[n+1],this.b=t[n+2],this}toArray(t=[],n=0){return t[n]=this.r,t[n+1]=this.g,t[n+2]=this.b,t}fromBufferAttribute(t,n){return this.r=t.getX(n),this.g=t.getY(n),this.b=t.getZ(n),this}toJSON(){return this.getHex()}*[Symbol.iterator](){yield this.r,yield this.g,yield this.b}}const Hn=new _e;_e.NAMES=Px;class kb extends kn{constructor(){super(),this.isScene=!0,this.type="Scene",this.background=null,this.environment=null,this.fog=null,this.backgroundBlurriness=0,this.backgroundIntensity=1,this.backgroundRotation=new xs,this.environmentIntensity=1,this.environmentRotation=new xs,this.overrideMaterial=null,typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("observe",{detail:this}))}copy(t,n){return super.copy(t,n),t.background!==null&&(this.background=t.background.clone()),t.environment!==null&&(this.environment=t.environment.clone()),t.fog!==null&&(this.fog=t.fog.clone()),this.backgroundBlurriness=t.backgroundBlurriness,this.backgroundIntensity=t.backgroundIntensity,this.backgroundRotation.copy(t.backgroundRotation),this.environmentIntensity=t.environmentIntensity,this.environmentRotation.copy(t.environmentRotation),t.overrideMaterial!==null&&(this.overrideMaterial=t.overrideMaterial.clone()),this.matrixAutoUpdate=t.matrixAutoUpdate,this}toJSON(t){const n=super.toJSON(t);return this.fog!==null&&(n.object.fog=this.fog.toJSON()),this.backgroundBlurriness>0&&(n.object.backgroundBlurriness=this.backgroundBlurriness),this.backgroundIntensity!==1&&(n.object.backgroundIntensity=this.backgroundIntensity),n.object.backgroundRotation=this.backgroundRotation.toArray(),this.environmentIntensity!==1&&(n.object.environmentIntensity=this.environmentIntensity),n.object.environmentRotation=this.environmentRotation.toArray(),n}}const zi=new k,Ma=new k,oh=new k,ba=new k,Fr=new k,Hr=new k,dv=new k,lh=new k,ch=new k,uh=new k,fh=new fn,dh=new fn,hh=new fn;class Ri{constructor(t=new k,n=new k,a=new k){this.a=t,this.b=n,this.c=a}static getNormal(t,n,a,l){l.subVectors(a,n),zi.subVectors(t,n),l.cross(zi);const c=l.lengthSq();return c>0?l.multiplyScalar(1/Math.sqrt(c)):l.set(0,0,0)}static getBarycoord(t,n,a,l,c){zi.subVectors(l,n),Ma.subVectors(a,n),oh.subVectors(t,n);const f=zi.dot(zi),d=zi.dot(Ma),m=zi.dot(oh),h=Ma.dot(Ma),g=Ma.dot(oh),_=f*h-d*d;if(_===0)return c.set(0,0,0),null;const v=1/_,y=(h*m-d*g)*v,E=(f*g-d*m)*v;return c.set(1-y-E,E,y)}static containsPoint(t,n,a,l){return this.getBarycoord(t,n,a,l,ba)===null?!1:ba.x>=0&&ba.y>=0&&ba.x+ba.y<=1}static getInterpolation(t,n,a,l,c,f,d,m){return this.getBarycoord(t,n,a,l,ba)===null?(m.x=0,m.y=0,"z"in m&&(m.z=0),"w"in m&&(m.w=0),null):(m.setScalar(0),m.addScaledVector(c,ba.x),m.addScaledVector(f,ba.y),m.addScaledVector(d,ba.z),m)}static getInterpolatedAttribute(t,n,a,l,c,f){return fh.setScalar(0),dh.setScalar(0),hh.setScalar(0),fh.fromBufferAttribute(t,n),dh.fromBufferAttribute(t,a),hh.fromBufferAttribute(t,l),f.setScalar(0),f.addScaledVector(fh,c.x),f.addScaledVector(dh,c.y),f.addScaledVector(hh,c.z),f}static isFrontFacing(t,n,a,l){return zi.subVectors(a,n),Ma.subVectors(t,n),zi.cross(Ma).dot(l)<0}set(t,n,a){return this.a.copy(t),this.b.copy(n),this.c.copy(a),this}setFromPointsAndIndices(t,n,a,l){return this.a.copy(t[n]),this.b.copy(t[a]),this.c.copy(t[l]),this}setFromAttributeAndIndices(t,n,a,l){return this.a.fromBufferAttribute(t,n),this.b.fromBufferAttribute(t,a),this.c.fromBufferAttribute(t,l),this}clone(){return new this.constructor().copy(this)}copy(t){return this.a.copy(t.a),this.b.copy(t.b),this.c.copy(t.c),this}getArea(){return zi.subVectors(this.c,this.b),Ma.subVectors(this.a,this.b),zi.cross(Ma).length()*.5}getMidpoint(t){return t.addVectors(this.a,this.b).add(this.c).multiplyScalar(1/3)}getNormal(t){return Ri.getNormal(this.a,this.b,this.c,t)}getPlane(t){return t.setFromCoplanarPoints(this.a,this.b,this.c)}getBarycoord(t,n){return Ri.getBarycoord(t,this.a,this.b,this.c,n)}getInterpolation(t,n,a,l,c){return Ri.getInterpolation(t,this.a,this.b,this.c,n,a,l,c)}containsPoint(t){return Ri.containsPoint(t,this.a,this.b,this.c)}isFrontFacing(t){return Ri.isFrontFacing(this.a,this.b,this.c,t)}intersectsBox(t){return t.intersectsTriangle(this)}closestPointToPoint(t,n){const a=this.a,l=this.b,c=this.c;let f,d;Fr.subVectors(l,a),Hr.subVectors(c,a),lh.subVectors(t,a);const m=Fr.dot(lh),h=Hr.dot(lh);if(m<=0&&h<=0)return n.copy(a);ch.subVectors(t,l);const g=Fr.dot(ch),_=Hr.dot(ch);if(g>=0&&_<=g)return n.copy(l);const v=m*_-g*h;if(v<=0&&m>=0&&g<=0)return f=m/(m-g),n.copy(a).addScaledVector(Fr,f);uh.subVectors(t,c);const y=Fr.dot(uh),E=Hr.dot(uh);if(E>=0&&y<=E)return n.copy(c);const A=y*h-m*E;if(A<=0&&h>=0&&E<=0)return d=h/(h-E),n.copy(a).addScaledVector(Hr,d);const S=g*E-y*_;if(S<=0&&_-g>=0&&y-E>=0)return dv.subVectors(c,l),d=(_-g)/(_-g+(y-E)),n.copy(l).addScaledVector(dv,d);const x=1/(S+A+v);return f=A*x,d=v*x,n.copy(a).addScaledVector(Fr,f).addScaledVector(Hr,d)}equals(t){return t.a.equals(this.a)&&t.b.equals(this.b)&&t.c.equals(this.c)}}class wl{constructor(t=new k(1/0,1/0,1/0),n=new k(-1/0,-1/0,-1/0)){this.isBox3=!0,this.min=t,this.max=n}set(t,n){return this.min.copy(t),this.max.copy(n),this}setFromArray(t){this.makeEmpty();for(let n=0,a=t.length;n<a;n+=3)this.expandByPoint(Bi.fromArray(t,n));return this}setFromBufferAttribute(t){this.makeEmpty();for(let n=0,a=t.count;n<a;n++)this.expandByPoint(Bi.fromBufferAttribute(t,n));return this}setFromPoints(t){this.makeEmpty();for(let n=0,a=t.length;n<a;n++)this.expandByPoint(t[n]);return this}setFromCenterAndSize(t,n){const a=Bi.copy(n).multiplyScalar(.5);return this.min.copy(t).sub(a),this.max.copy(t).add(a),this}setFromObject(t,n=!1){return this.makeEmpty(),this.expandByObject(t,n)}clone(){return new this.constructor().copy(this)}copy(t){return this.min.copy(t.min),this.max.copy(t.max),this}makeEmpty(){return this.min.x=this.min.y=this.min.z=1/0,this.max.x=this.max.y=this.max.z=-1/0,this}isEmpty(){return this.max.x<this.min.x||this.max.y<this.min.y||this.max.z<this.min.z}getCenter(t){return this.isEmpty()?t.set(0,0,0):t.addVectors(this.min,this.max).multiplyScalar(.5)}getSize(t){return this.isEmpty()?t.set(0,0,0):t.subVectors(this.max,this.min)}expandByPoint(t){return this.min.min(t),this.max.max(t),this}expandByVector(t){return this.min.sub(t),this.max.add(t),this}expandByScalar(t){return this.min.addScalar(-t),this.max.addScalar(t),this}expandByObject(t,n=!1){t.updateWorldMatrix(!1,!1);const a=t.geometry;if(a!==void 0){const c=a.getAttribute("position");if(n===!0&&c!==void 0&&t.isInstancedMesh!==!0)for(let f=0,d=c.count;f<d;f++)t.isMesh===!0?t.getVertexPosition(f,Bi):Bi.fromBufferAttribute(c,f),Bi.applyMatrix4(t.matrixWorld),this.expandByPoint(Bi);else t.boundingBox!==void 0?(t.boundingBox===null&&t.computeBoundingBox(),kc.copy(t.boundingBox)):(a.boundingBox===null&&a.computeBoundingBox(),kc.copy(a.boundingBox)),kc.applyMatrix4(t.matrixWorld),this.union(kc)}const l=t.children;for(let c=0,f=l.length;c<f;c++)this.expandByObject(l[c],n);return this}containsPoint(t){return t.x>=this.min.x&&t.x<=this.max.x&&t.y>=this.min.y&&t.y<=this.max.y&&t.z>=this.min.z&&t.z<=this.max.z}containsBox(t){return this.min.x<=t.min.x&&t.max.x<=this.max.x&&this.min.y<=t.min.y&&t.max.y<=this.max.y&&this.min.z<=t.min.z&&t.max.z<=this.max.z}getParameter(t,n){return n.set((t.x-this.min.x)/(this.max.x-this.min.x),(t.y-this.min.y)/(this.max.y-this.min.y),(t.z-this.min.z)/(this.max.z-this.min.z))}intersectsBox(t){return t.max.x>=this.min.x&&t.min.x<=this.max.x&&t.max.y>=this.min.y&&t.min.y<=this.max.y&&t.max.z>=this.min.z&&t.min.z<=this.max.z}intersectsSphere(t){return this.clampPoint(t.center,Bi),Bi.distanceToSquared(t.center)<=t.radius*t.radius}intersectsPlane(t){let n,a;return t.normal.x>0?(n=t.normal.x*this.min.x,a=t.normal.x*this.max.x):(n=t.normal.x*this.max.x,a=t.normal.x*this.min.x),t.normal.y>0?(n+=t.normal.y*this.min.y,a+=t.normal.y*this.max.y):(n+=t.normal.y*this.max.y,a+=t.normal.y*this.min.y),t.normal.z>0?(n+=t.normal.z*this.min.z,a+=t.normal.z*this.max.z):(n+=t.normal.z*this.max.z,a+=t.normal.z*this.min.z),n<=-t.constant&&a>=-t.constant}intersectsTriangle(t){if(this.isEmpty())return!1;this.getCenter(rl),jc.subVectors(this.max,rl),Gr.subVectors(t.a,rl),Vr.subVectors(t.b,rl),kr.subVectors(t.c,rl),fs.subVectors(Vr,Gr),ds.subVectors(kr,Vr),Fs.subVectors(Gr,kr);let n=[0,-fs.z,fs.y,0,-ds.z,ds.y,0,-Fs.z,Fs.y,fs.z,0,-fs.x,ds.z,0,-ds.x,Fs.z,0,-Fs.x,-fs.y,fs.x,0,-ds.y,ds.x,0,-Fs.y,Fs.x,0];return!ph(n,Gr,Vr,kr,jc)||(n=[1,0,0,0,1,0,0,0,1],!ph(n,Gr,Vr,kr,jc))?!1:(Xc.crossVectors(fs,ds),n=[Xc.x,Xc.y,Xc.z],ph(n,Gr,Vr,kr,jc))}clampPoint(t,n){return n.copy(t).clamp(this.min,this.max)}distanceToPoint(t){return this.clampPoint(t,Bi).distanceTo(t)}getBoundingSphere(t){return this.isEmpty()?t.makeEmpty():(this.getCenter(t.center),t.radius=this.getSize(Bi).length()*.5),t}intersect(t){return this.min.max(t.min),this.max.min(t.max),this.isEmpty()&&this.makeEmpty(),this}union(t){return this.min.min(t.min),this.max.max(t.max),this}applyMatrix4(t){return this.isEmpty()?this:(Ea[0].set(this.min.x,this.min.y,this.min.z).applyMatrix4(t),Ea[1].set(this.min.x,this.min.y,this.max.z).applyMatrix4(t),Ea[2].set(this.min.x,this.max.y,this.min.z).applyMatrix4(t),Ea[3].set(this.min.x,this.max.y,this.max.z).applyMatrix4(t),Ea[4].set(this.max.x,this.min.y,this.min.z).applyMatrix4(t),Ea[5].set(this.max.x,this.min.y,this.max.z).applyMatrix4(t),Ea[6].set(this.max.x,this.max.y,this.min.z).applyMatrix4(t),Ea[7].set(this.max.x,this.max.y,this.max.z).applyMatrix4(t),this.setFromPoints(Ea),this)}translate(t){return this.min.add(t),this.max.add(t),this}equals(t){return t.min.equals(this.min)&&t.max.equals(this.max)}toJSON(){return{min:this.min.toArray(),max:this.max.toArray()}}fromJSON(t){return this.min.fromArray(t.min),this.max.fromArray(t.max),this}}const Ea=[new k,new k,new k,new k,new k,new k,new k,new k],Bi=new k,kc=new wl,Gr=new k,Vr=new k,kr=new k,fs=new k,ds=new k,Fs=new k,rl=new k,jc=new k,Xc=new k,Hs=new k;function ph(r,t,n,a,l){for(let c=0,f=r.length-3;c<=f;c+=3){Hs.fromArray(r,c);const d=l.x*Math.abs(Hs.x)+l.y*Math.abs(Hs.y)+l.z*Math.abs(Hs.z),m=t.dot(Hs),h=n.dot(Hs),g=a.dot(Hs);if(Math.max(-Math.max(m,h,g),Math.min(m,h,g))>d)return!1}return!0}const yn=new k,Wc=new ee;let jb=0;class Ci extends tr{constructor(t,n,a=!1){if(super(),Array.isArray(t))throw new TypeError("THREE.BufferAttribute: array should be a Typed Array.");this.isBufferAttribute=!0,Object.defineProperty(this,"id",{value:jb++}),this.name="",this.array=t,this.itemSize=n,this.count=t!==void 0?t.length/n:0,this.normalized=a,this.usage=Ep,this.updateRanges=[],this.gpuType=Qi,this.version=0}onUploadCallback(){}set needsUpdate(t){t===!0&&this.version++}setUsage(t){return this.usage=t,this}addUpdateRange(t,n){this.updateRanges.push({start:t,count:n})}clearUpdateRanges(){this.updateRanges.length=0}copy(t){return this.name=t.name,this.array=new t.array.constructor(t.array),this.itemSize=t.itemSize,this.count=t.count,this.normalized=t.normalized,this.usage=t.usage,this.gpuType=t.gpuType,this}copyAt(t,n,a){t*=this.itemSize,a*=n.itemSize;for(let l=0,c=this.itemSize;l<c;l++)this.array[t+l]=n.array[a+l];return this}copyArray(t){return this.array.set(t),this}applyMatrix3(t){if(this.itemSize===2)for(let n=0,a=this.count;n<a;n++)Wc.fromBufferAttribute(this,n),Wc.applyMatrix3(t),this.setXY(n,Wc.x,Wc.y);else if(this.itemSize===3)for(let n=0,a=this.count;n<a;n++)yn.fromBufferAttribute(this,n),yn.applyMatrix3(t),this.setXYZ(n,yn.x,yn.y,yn.z);return this}applyMatrix4(t){for(let n=0,a=this.count;n<a;n++)yn.fromBufferAttribute(this,n),yn.applyMatrix4(t),this.setXYZ(n,yn.x,yn.y,yn.z);return this}applyNormalMatrix(t){for(let n=0,a=this.count;n<a;n++)yn.fromBufferAttribute(this,n),yn.applyNormalMatrix(t),this.setXYZ(n,yn.x,yn.y,yn.z);return this}transformDirection(t){for(let n=0,a=this.count;n<a;n++)yn.fromBufferAttribute(this,n),yn.transformDirection(t),this.setXYZ(n,yn.x,yn.y,yn.z);return this}set(t,n=0){return this.array.set(t,n),this}getComponent(t,n){let a=this.array[t*this.itemSize+n];return this.normalized&&(a=Fi(a,this.array)),a}setComponent(t,n,a){return this.normalized&&(a=Xe(a,this.array)),this.array[t*this.itemSize+n]=a,this}getX(t){let n=this.array[t*this.itemSize];return this.normalized&&(n=Fi(n,this.array)),n}setX(t,n){return this.normalized&&(n=Xe(n,this.array)),this.array[t*this.itemSize]=n,this}getY(t){let n=this.array[t*this.itemSize+1];return this.normalized&&(n=Fi(n,this.array)),n}setY(t,n){return this.normalized&&(n=Xe(n,this.array)),this.array[t*this.itemSize+1]=n,this}getZ(t){let n=this.array[t*this.itemSize+2];return this.normalized&&(n=Fi(n,this.array)),n}setZ(t,n){return this.normalized&&(n=Xe(n,this.array)),this.array[t*this.itemSize+2]=n,this}getW(t){let n=this.array[t*this.itemSize+3];return this.normalized&&(n=Fi(n,this.array)),n}setW(t,n){return this.normalized&&(n=Xe(n,this.array)),this.array[t*this.itemSize+3]=n,this}setXY(t,n,a){return t*=this.itemSize,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array)),this.array[t+0]=n,this.array[t+1]=a,this}setXYZ(t,n,a,l){return t*=this.itemSize,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array),l=Xe(l,this.array)),this.array[t+0]=n,this.array[t+1]=a,this.array[t+2]=l,this}setXYZW(t,n,a,l,c){return t*=this.itemSize,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array),l=Xe(l,this.array),c=Xe(c,this.array)),this.array[t+0]=n,this.array[t+1]=a,this.array[t+2]=l,this.array[t+3]=c,this}onUpload(t){return this.onUploadCallback=t,this}clone(){return new this.constructor(this.array,this.itemSize).copy(this)}toJSON(){const t={itemSize:this.itemSize,type:this.array.constructor.name,array:Array.from(this.array),normalized:this.normalized};return this.name!==""&&(t.name=this.name),this.usage!==Ep&&(t.usage=this.usage),t}dispose(){this.dispatchEvent({type:"dispose"})}}class Ix extends Ci{constructor(t,n,a){super(new Uint16Array(t),n,a)}}class zx extends Ci{constructor(t,n,a){super(new Uint32Array(t),n,a)}}class Rn extends Ci{constructor(t,n,a){super(new Float32Array(t),n,a)}}const Xb=new wl,ol=new k,mh=new k;class Ou{constructor(t=new k,n=-1){this.isSphere=!0,this.center=t,this.radius=n}set(t,n){return this.center.copy(t),this.radius=n,this}setFromPoints(t,n){const a=this.center;n!==void 0?a.copy(n):Xb.setFromPoints(t).getCenter(a);let l=0;for(let c=0,f=t.length;c<f;c++)l=Math.max(l,a.distanceToSquared(t[c]));return this.radius=Math.sqrt(l),this}copy(t){return this.center.copy(t.center),this.radius=t.radius,this}isEmpty(){return this.radius<0}makeEmpty(){return this.center.set(0,0,0),this.radius=-1,this}containsPoint(t){return t.distanceToSquared(this.center)<=this.radius*this.radius}distanceToPoint(t){return t.distanceTo(this.center)-this.radius}intersectsSphere(t){const n=this.radius+t.radius;return t.center.distanceToSquared(this.center)<=n*n}intersectsBox(t){return t.intersectsSphere(this)}intersectsPlane(t){return Math.abs(t.distanceToPoint(this.center))<=this.radius}clampPoint(t,n){const a=this.center.distanceToSquared(t);return n.copy(t),a>this.radius*this.radius&&(n.sub(this.center).normalize(),n.multiplyScalar(this.radius).add(this.center)),n}getBoundingBox(t){return this.isEmpty()?(t.makeEmpty(),t):(t.set(this.center,this.center),t.expandByScalar(this.radius),t)}applyMatrix4(t){return this.center.applyMatrix4(t),this.radius=this.radius*t.getMaxScaleOnAxis(),this}translate(t){return this.center.add(t),this}expandByPoint(t){if(this.isEmpty())return this.center.copy(t),this.radius=0,this;ol.subVectors(t,this.center);const n=ol.lengthSq();if(n>this.radius*this.radius){const a=Math.sqrt(n),l=(a-this.radius)*.5;this.center.addScaledVector(ol,l/a),this.radius+=l}return this}union(t){return t.isEmpty()?this:this.isEmpty()?(this.copy(t),this):(this.center.equals(t.center)===!0?this.radius=Math.max(this.radius,t.radius):(mh.subVectors(t.center,this.center).setLength(t.radius),this.expandByPoint(ol.copy(t.center).add(mh)),this.expandByPoint(ol.copy(t.center).sub(mh))),this)}equals(t){return t.center.equals(this.center)&&t.radius===this.radius}clone(){return new this.constructor().copy(this)}toJSON(){return{radius:this.radius,center:this.center.toArray()}}fromJSON(t){return this.radius=t.radius,this.center.fromArray(t.center),this}}let Wb=0;const Ei=new tn,gh=new kn,jr=new k,di=new wl,ll=new wl,wn=new k;class qn extends tr{constructor(){super(),this.isBufferGeometry=!0,Object.defineProperty(this,"id",{value:Wb++}),this.uuid=Ca(),this.name="",this.type="BufferGeometry",this.index=null,this.indirect=null,this.indirectOffset=0,this.attributes={},this.morphAttributes={},this.morphTargetsRelative=!1,this.groups=[],this.boundingBox=null,this.boundingSphere=null,this.drawRange={start:0,count:1/0},this.userData={}}getIndex(){return this.index}setIndex(t){return Array.isArray(t)?this.index=new(ub(t)?zx:Ix)(t,1):this.index=t,this}setIndirect(t,n=0){return this.indirect=t,this.indirectOffset=n,this}getIndirect(){return this.indirect}getAttribute(t){return this.attributes[t]}setAttribute(t,n){return this.attributes[t]=n,this}deleteAttribute(t){return delete this.attributes[t],this}hasAttribute(t){return this.attributes[t]!==void 0}addGroup(t,n,a=0){this.groups.push({start:t,count:n,materialIndex:a})}clearGroups(){this.groups=[]}setDrawRange(t,n){this.drawRange.start=t,this.drawRange.count=n}applyMatrix4(t){const n=this.attributes.position;n!==void 0&&(n.applyMatrix4(t),n.needsUpdate=!0);const a=this.attributes.normal;if(a!==void 0){const c=new pe().getNormalMatrix(t);a.applyNormalMatrix(c),a.needsUpdate=!0}const l=this.attributes.tangent;return l!==void 0&&(l.transformDirection(t),l.needsUpdate=!0),this.boundingBox!==null&&this.computeBoundingBox(),this.boundingSphere!==null&&this.computeBoundingSphere(),this}applyQuaternion(t){return Ei.makeRotationFromQuaternion(t),this.applyMatrix4(Ei),this}rotateX(t){return Ei.makeRotationX(t),this.applyMatrix4(Ei),this}rotateY(t){return Ei.makeRotationY(t),this.applyMatrix4(Ei),this}rotateZ(t){return Ei.makeRotationZ(t),this.applyMatrix4(Ei),this}translate(t,n,a){return Ei.makeTranslation(t,n,a),this.applyMatrix4(Ei),this}scale(t,n,a){return Ei.makeScale(t,n,a),this.applyMatrix4(Ei),this}lookAt(t){return gh.lookAt(t),gh.updateMatrix(),this.applyMatrix4(gh.matrix),this}center(){return this.computeBoundingBox(),this.boundingBox.getCenter(jr).negate(),this.translate(jr.x,jr.y,jr.z),this}setFromPoints(t){const n=this.getAttribute("position");if(n===void 0){const a=[];for(let l=0,c=t.length;l<c;l++){const f=t[l];a.push(f.x,f.y,f.z||0)}this.setAttribute("position",new Rn(a,3))}else{const a=Math.min(t.length,n.count);for(let l=0;l<a;l++){const c=t[l];n.setXYZ(l,c.x,c.y,c.z||0)}t.length>n.count&&ce("BufferGeometry: Buffer size too small for points data. Use .dispose() and create a new geometry."),n.needsUpdate=!0}return this}computeBoundingBox(){this.boundingBox===null&&(this.boundingBox=new wl);const t=this.attributes.position,n=this.morphAttributes.position;if(t&&t.isGLBufferAttribute){Ne("BufferGeometry.computeBoundingBox(): GLBufferAttribute requires a manual bounding box.",this),this.boundingBox.set(new k(-1/0,-1/0,-1/0),new k(1/0,1/0,1/0));return}if(t!==void 0){if(this.boundingBox.setFromBufferAttribute(t),n)for(let a=0,l=n.length;a<l;a++){const c=n[a];di.setFromBufferAttribute(c),this.morphTargetsRelative?(wn.addVectors(this.boundingBox.min,di.min),this.boundingBox.expandByPoint(wn),wn.addVectors(this.boundingBox.max,di.max),this.boundingBox.expandByPoint(wn)):(this.boundingBox.expandByPoint(di.min),this.boundingBox.expandByPoint(di.max))}}else this.boundingBox.makeEmpty();(isNaN(this.boundingBox.min.x)||isNaN(this.boundingBox.min.y)||isNaN(this.boundingBox.min.z))&&Ne('BufferGeometry.computeBoundingBox(): Computed min/max have NaN values. The "position" attribute is likely to have NaN values.',this)}computeBoundingSphere(){this.boundingSphere===null&&(this.boundingSphere=new Ou);const t=this.attributes.position,n=this.morphAttributes.position;if(t&&t.isGLBufferAttribute){Ne("BufferGeometry.computeBoundingSphere(): GLBufferAttribute requires a manual bounding sphere.",this),this.boundingSphere.set(new k,1/0);return}if(t){const a=this.boundingSphere.center;if(di.setFromBufferAttribute(t),n)for(let c=0,f=n.length;c<f;c++){const d=n[c];ll.setFromBufferAttribute(d),this.morphTargetsRelative?(wn.addVectors(di.min,ll.min),di.expandByPoint(wn),wn.addVectors(di.max,ll.max),di.expandByPoint(wn)):(di.expandByPoint(ll.min),di.expandByPoint(ll.max))}di.getCenter(a);let l=0;for(let c=0,f=t.count;c<f;c++)wn.fromBufferAttribute(t,c),l=Math.max(l,a.distanceToSquared(wn));if(n)for(let c=0,f=n.length;c<f;c++){const d=n[c],m=this.morphTargetsRelative;for(let h=0,g=d.count;h<g;h++)wn.fromBufferAttribute(d,h),m&&(jr.fromBufferAttribute(t,h),wn.add(jr)),l=Math.max(l,a.distanceToSquared(wn))}this.boundingSphere.radius=Math.sqrt(l),isNaN(this.boundingSphere.radius)&&Ne('BufferGeometry.computeBoundingSphere(): Computed radius is NaN. The "position" attribute is likely to have NaN values.',this)}}computeTangents(){const t=this.index,n=this.attributes;if(t===null||n.position===void 0||n.normal===void 0||n.uv===void 0){Ne("BufferGeometry: .computeTangents() failed. Missing required attributes (index, position, normal or uv)");return}const a=n.position,l=n.normal,c=n.uv;this.hasAttribute("tangent")===!1&&this.setAttribute("tangent",new Ci(new Float32Array(4*a.count),4));const f=this.getAttribute("tangent"),d=[],m=[];for(let R=0;R<a.count;R++)d[R]=new k,m[R]=new k;const h=new k,g=new k,_=new k,v=new ee,y=new ee,E=new ee,A=new k,S=new k;function x(R,z,K){h.fromBufferAttribute(a,R),g.fromBufferAttribute(a,z),_.fromBufferAttribute(a,K),v.fromBufferAttribute(c,R),y.fromBufferAttribute(c,z),E.fromBufferAttribute(c,K),g.sub(h),_.sub(h),y.sub(v),E.sub(v);const V=1/(y.x*E.y-E.x*y.y);isFinite(V)&&(A.copy(g).multiplyScalar(E.y).addScaledVector(_,-y.y).multiplyScalar(V),S.copy(_).multiplyScalar(y.x).addScaledVector(g,-E.x).multiplyScalar(V),d[R].add(A),d[z].add(A),d[K].add(A),m[R].add(S),m[z].add(S),m[K].add(S))}let w=this.groups;w.length===0&&(w=[{start:0,count:t.count}]);for(let R=0,z=w.length;R<z;++R){const K=w[R],V=K.start,$=K.count;for(let ht=V,gt=V+$;ht<gt;ht+=3)x(t.getX(ht+0),t.getX(ht+1),t.getX(ht+2))}const D=new k,U=new k,G=new k,O=new k;function B(R){G.fromBufferAttribute(l,R),O.copy(G);const z=d[R];D.copy(z),D.sub(G.multiplyScalar(G.dot(z))).normalize(),U.crossVectors(O,z);const V=U.dot(m[R])<0?-1:1;f.setXYZW(R,D.x,D.y,D.z,V)}for(let R=0,z=w.length;R<z;++R){const K=w[R],V=K.start,$=K.count;for(let ht=V,gt=V+$;ht<gt;ht+=3)B(t.getX(ht+0)),B(t.getX(ht+1)),B(t.getX(ht+2))}}computeVertexNormals(){const t=this.index,n=this.getAttribute("position");if(n!==void 0){let a=this.getAttribute("normal");if(a===void 0)a=new Ci(new Float32Array(n.count*3),3),this.setAttribute("normal",a);else for(let v=0,y=a.count;v<y;v++)a.setXYZ(v,0,0,0);const l=new k,c=new k,f=new k,d=new k,m=new k,h=new k,g=new k,_=new k;if(t)for(let v=0,y=t.count;v<y;v+=3){const E=t.getX(v+0),A=t.getX(v+1),S=t.getX(v+2);l.fromBufferAttribute(n,E),c.fromBufferAttribute(n,A),f.fromBufferAttribute(n,S),g.subVectors(f,c),_.subVectors(l,c),g.cross(_),d.fromBufferAttribute(a,E),m.fromBufferAttribute(a,A),h.fromBufferAttribute(a,S),d.add(g),m.add(g),h.add(g),a.setXYZ(E,d.x,d.y,d.z),a.setXYZ(A,m.x,m.y,m.z),a.setXYZ(S,h.x,h.y,h.z)}else for(let v=0,y=n.count;v<y;v+=3)l.fromBufferAttribute(n,v+0),c.fromBufferAttribute(n,v+1),f.fromBufferAttribute(n,v+2),g.subVectors(f,c),_.subVectors(l,c),g.cross(_),a.setXYZ(v+0,g.x,g.y,g.z),a.setXYZ(v+1,g.x,g.y,g.z),a.setXYZ(v+2,g.x,g.y,g.z);this.normalizeNormals(),a.needsUpdate=!0}}normalizeNormals(){const t=this.attributes.normal;for(let n=0,a=t.count;n<a;n++)wn.fromBufferAttribute(t,n),wn.normalize(),t.setXYZ(n,wn.x,wn.y,wn.z)}toNonIndexed(){function t(d,m){const h=d.array,g=d.itemSize,_=d.normalized,v=new h.constructor(m.length*g);let y=0,E=0;for(let A=0,S=m.length;A<S;A++){d.isInterleavedBufferAttribute?y=m[A]*d.data.stride+d.offset:y=m[A]*g;for(let x=0;x<g;x++)v[E++]=h[y++]}return new Ci(v,g,_)}if(this.index===null)return ce("BufferGeometry.toNonIndexed(): BufferGeometry is already non-indexed."),this;const n=new qn,a=this.index.array,l=this.attributes;for(const d in l){const m=l[d],h=t(m,a);n.setAttribute(d,h)}const c=this.morphAttributes;for(const d in c){const m=[],h=c[d];for(let g=0,_=h.length;g<_;g++){const v=h[g],y=t(v,a);m.push(y)}n.morphAttributes[d]=m}n.morphTargetsRelative=this.morphTargetsRelative;const f=this.groups;for(let d=0,m=f.length;d<m;d++){const h=f[d];n.addGroup(h.start,h.count,h.materialIndex)}return n}toJSON(){const t={metadata:{version:4.7,type:"BufferGeometry",generator:"BufferGeometry.toJSON"}};if(t.uuid=this.uuid,t.type=this.type,this.name!==""&&(t.name=this.name),Object.keys(this.userData).length>0&&(t.userData=this.userData),this.parameters!==void 0){const m=this.parameters;for(const h in m)m[h]!==void 0&&(t[h]=m[h]);return t}t.data={attributes:{}};const n=this.index;n!==null&&(t.data.index={type:n.array.constructor.name,array:Array.prototype.slice.call(n.array)});const a=this.attributes;for(const m in a){const h=a[m];t.data.attributes[m]=h.toJSON(t.data)}const l={};let c=!1;for(const m in this.morphAttributes){const h=this.morphAttributes[m],g=[];for(let _=0,v=h.length;_<v;_++){const y=h[_];g.push(y.toJSON(t.data))}g.length>0&&(l[m]=g,c=!0)}c&&(t.data.morphAttributes=l,t.data.morphTargetsRelative=this.morphTargetsRelative);const f=this.groups;f.length>0&&(t.data.groups=JSON.parse(JSON.stringify(f)));const d=this.boundingSphere;return d!==null&&(t.data.boundingSphere=d.toJSON()),t}clone(){return new this.constructor().copy(this)}copy(t){this.index=null,this.attributes={},this.morphAttributes={},this.groups=[],this.boundingBox=null,this.boundingSphere=null;const n={};this.name=t.name;const a=t.index;a!==null&&this.setIndex(a.clone());const l=t.attributes;for(const h in l){const g=l[h];this.setAttribute(h,g.clone(n))}const c=t.morphAttributes;for(const h in c){const g=[],_=c[h];for(let v=0,y=_.length;v<y;v++)g.push(_[v].clone(n));this.morphAttributes[h]=g}this.morphTargetsRelative=t.morphTargetsRelative;const f=t.groups;for(let h=0,g=f.length;h<g;h++){const _=f[h];this.addGroup(_.start,_.count,_.materialIndex)}const d=t.boundingBox;d!==null&&(this.boundingBox=d.clone());const m=t.boundingSphere;return m!==null&&(this.boundingSphere=m.clone()),this.drawRange.start=t.drawRange.start,this.drawRange.count=t.drawRange.count,this.userData=t.userData,this}dispose(){this.dispatchEvent({type:"dispose"})}}class qb{constructor(t,n){this.isInterleavedBuffer=!0,this.array=t,this.stride=n,this.count=t!==void 0?t.length/n:0,this.usage=Ep,this.updateRanges=[],this.version=0,this.uuid=Ca()}onUploadCallback(){}set needsUpdate(t){t===!0&&this.version++}setUsage(t){return this.usage=t,this}addUpdateRange(t,n){this.updateRanges.push({start:t,count:n})}clearUpdateRanges(){this.updateRanges.length=0}copy(t){return this.array=new t.array.constructor(t.array),this.count=t.count,this.stride=t.stride,this.usage=t.usage,this}copyAt(t,n,a){t*=this.stride,a*=n.stride;for(let l=0,c=this.stride;l<c;l++)this.array[t+l]=n.array[a+l];return this}set(t,n=0){return this.array.set(t,n),this}clone(t){t.arrayBuffers===void 0&&(t.arrayBuffers={}),this.array.buffer._uuid===void 0&&(this.array.buffer._uuid=Ca()),t.arrayBuffers[this.array.buffer._uuid]===void 0&&(t.arrayBuffers[this.array.buffer._uuid]=this.array.slice(0).buffer);const n=new this.array.constructor(t.arrayBuffers[this.array.buffer._uuid]),a=new this.constructor(n,this.stride);return a.setUsage(this.usage),a}onUpload(t){return this.onUploadCallback=t,this}toJSON(t){return t.arrayBuffers===void 0&&(t.arrayBuffers={}),this.array.buffer._uuid===void 0&&(this.array.buffer._uuid=Ca()),t.arrayBuffers[this.array.buffer._uuid]===void 0&&(t.arrayBuffers[this.array.buffer._uuid]=Array.from(new Uint32Array(this.array.buffer))),{uuid:this.uuid,buffer:this.array.buffer._uuid,type:this.array.constructor.name,stride:this.stride}}}const Xn=new k;class Ru{constructor(t,n,a,l=!1){this.isInterleavedBufferAttribute=!0,this.name="",this.data=t,this.itemSize=n,this.offset=a,this.normalized=l}get count(){return this.data.count}get array(){return this.data.array}set needsUpdate(t){this.data.needsUpdate=t}applyMatrix4(t){for(let n=0,a=this.data.count;n<a;n++)Xn.fromBufferAttribute(this,n),Xn.applyMatrix4(t),this.setXYZ(n,Xn.x,Xn.y,Xn.z);return this}applyNormalMatrix(t){for(let n=0,a=this.count;n<a;n++)Xn.fromBufferAttribute(this,n),Xn.applyNormalMatrix(t),this.setXYZ(n,Xn.x,Xn.y,Xn.z);return this}transformDirection(t){for(let n=0,a=this.count;n<a;n++)Xn.fromBufferAttribute(this,n),Xn.transformDirection(t),this.setXYZ(n,Xn.x,Xn.y,Xn.z);return this}getComponent(t,n){let a=this.array[t*this.data.stride+this.offset+n];return this.normalized&&(a=Fi(a,this.array)),a}setComponent(t,n,a){return this.normalized&&(a=Xe(a,this.array)),this.data.array[t*this.data.stride+this.offset+n]=a,this}setX(t,n){return this.normalized&&(n=Xe(n,this.array)),this.data.array[t*this.data.stride+this.offset]=n,this}setY(t,n){return this.normalized&&(n=Xe(n,this.array)),this.data.array[t*this.data.stride+this.offset+1]=n,this}setZ(t,n){return this.normalized&&(n=Xe(n,this.array)),this.data.array[t*this.data.stride+this.offset+2]=n,this}setW(t,n){return this.normalized&&(n=Xe(n,this.array)),this.data.array[t*this.data.stride+this.offset+3]=n,this}getX(t){let n=this.data.array[t*this.data.stride+this.offset];return this.normalized&&(n=Fi(n,this.array)),n}getY(t){let n=this.data.array[t*this.data.stride+this.offset+1];return this.normalized&&(n=Fi(n,this.array)),n}getZ(t){let n=this.data.array[t*this.data.stride+this.offset+2];return this.normalized&&(n=Fi(n,this.array)),n}getW(t){let n=this.data.array[t*this.data.stride+this.offset+3];return this.normalized&&(n=Fi(n,this.array)),n}setXY(t,n,a){return t=t*this.data.stride+this.offset,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array)),this.data.array[t+0]=n,this.data.array[t+1]=a,this}setXYZ(t,n,a,l){return t=t*this.data.stride+this.offset,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array),l=Xe(l,this.array)),this.data.array[t+0]=n,this.data.array[t+1]=a,this.data.array[t+2]=l,this}setXYZW(t,n,a,l,c){return t=t*this.data.stride+this.offset,this.normalized&&(n=Xe(n,this.array),a=Xe(a,this.array),l=Xe(l,this.array),c=Xe(c,this.array)),this.data.array[t+0]=n,this.data.array[t+1]=a,this.data.array[t+2]=l,this.data.array[t+3]=c,this}clone(t){if(t===void 0){wu("InterleavedBufferAttribute.clone(): Cloning an interleaved buffer attribute will de-interleave buffer data.");const n=[];for(let a=0;a<this.count;a++){const l=a*this.data.stride+this.offset;for(let c=0;c<this.itemSize;c++)n.push(this.data.array[l+c])}return new Ci(new this.array.constructor(n),this.itemSize,this.normalized)}else return t.interleavedBuffers===void 0&&(t.interleavedBuffers={}),t.interleavedBuffers[this.data.uuid]===void 0&&(t.interleavedBuffers[this.data.uuid]=this.data.clone(t)),new Ru(t.interleavedBuffers[this.data.uuid],this.itemSize,this.offset,this.normalized)}toJSON(t){if(t===void 0){wu("InterleavedBufferAttribute.toJSON(): Serializing an interleaved buffer attribute will de-interleave buffer data.");const n=[];for(let a=0;a<this.count;a++){const l=a*this.data.stride+this.offset;for(let c=0;c<this.itemSize;c++)n.push(this.data.array[l+c])}return{itemSize:this.itemSize,type:this.array.constructor.name,array:n,normalized:this.normalized}}else return t.interleavedBuffers===void 0&&(t.interleavedBuffers={}),t.interleavedBuffers[this.data.uuid]===void 0&&(t.interleavedBuffers[this.data.uuid]=this.data.toJSON(t)),{isInterleavedBufferAttribute:!0,itemSize:this.itemSize,data:this.data.uuid,offset:this.offset,normalized:this.normalized}}}let Yb=0;class er extends tr{constructor(){super(),this.isMaterial=!0,Object.defineProperty(this,"id",{value:Yb++}),this.uuid=Ca(),this.name="",this.type="Material",this.blending=io,this.side=vs,this.vertexColors=!1,this.opacity=1,this.transparent=!1,this.alphaHash=!1,this.blendSrc=Ih,this.blendDst=zh,this.blendEquation=Xs,this.blendSrcAlpha=null,this.blendDstAlpha=null,this.blendEquationAlpha=null,this.blendColor=new _e(0,0,0),this.blendAlpha=0,this.depthFunc=so,this.depthTest=!0,this.depthWrite=!0,this.stencilWriteMask=255,this.stencilFunc=J_,this.stencilRef=0,this.stencilFuncMask=255,this.stencilFail=Or,this.stencilZFail=Or,this.stencilZPass=Or,this.stencilWrite=!1,this.clippingPlanes=null,this.clipIntersection=!1,this.clipShadows=!1,this.shadowSide=null,this.colorWrite=!0,this.precision=null,this.polygonOffset=!1,this.polygonOffsetFactor=0,this.polygonOffsetUnits=0,this.dithering=!1,this.alphaToCoverage=!1,this.premultipliedAlpha=!1,this.forceSinglePass=!1,this.allowOverride=!0,this.visible=!0,this.toneMapped=!0,this.userData={},this.version=0,this._alphaTest=0}get alphaTest(){return this._alphaTest}set alphaTest(t){this._alphaTest>0!=t>0&&this.version++,this._alphaTest=t}onBeforeRender(){}onBeforeCompile(){}customProgramCacheKey(){return this.onBeforeCompile.toString()}setValues(t){if(t!==void 0)for(const n in t){const a=t[n];if(a===void 0){ce(`Material: parameter '${n}' has value of undefined.`);continue}const l=this[n];if(l===void 0){ce(`Material: '${n}' is not a property of THREE.${this.type}.`);continue}l&&l.isColor?l.set(a):l&&l.isVector3&&a&&a.isVector3?l.copy(a):this[n]=a}}toJSON(t){const n=t===void 0||typeof t=="string";n&&(t={textures:{},images:{}});const a={metadata:{version:4.7,type:"Material",generator:"Material.toJSON"}};a.uuid=this.uuid,a.type=this.type,this.name!==""&&(a.name=this.name),this.color&&this.color.isColor&&(a.color=this.color.getHex()),this.roughness!==void 0&&(a.roughness=this.roughness),this.metalness!==void 0&&(a.metalness=this.metalness),this.sheen!==void 0&&(a.sheen=this.sheen),this.sheenColor&&this.sheenColor.isColor&&(a.sheenColor=this.sheenColor.getHex()),this.sheenRoughness!==void 0&&(a.sheenRoughness=this.sheenRoughness),this.emissive&&this.emissive.isColor&&(a.emissive=this.emissive.getHex()),this.emissiveIntensity!==void 0&&this.emissiveIntensity!==1&&(a.emissiveIntensity=this.emissiveIntensity),this.specular&&this.specular.isColor&&(a.specular=this.specular.getHex()),this.specularIntensity!==void 0&&(a.specularIntensity=this.specularIntensity),this.specularColor&&this.specularColor.isColor&&(a.specularColor=this.specularColor.getHex()),this.shininess!==void 0&&(a.shininess=this.shininess),this.clearcoat!==void 0&&(a.clearcoat=this.clearcoat),this.clearcoatRoughness!==void 0&&(a.clearcoatRoughness=this.clearcoatRoughness),this.clearcoatMap&&this.clearcoatMap.isTexture&&(a.clearcoatMap=this.clearcoatMap.toJSON(t).uuid),this.clearcoatRoughnessMap&&this.clearcoatRoughnessMap.isTexture&&(a.clearcoatRoughnessMap=this.clearcoatRoughnessMap.toJSON(t).uuid),this.clearcoatNormalMap&&this.clearcoatNormalMap.isTexture&&(a.clearcoatNormalMap=this.clearcoatNormalMap.toJSON(t).uuid,a.clearcoatNormalScale=this.clearcoatNormalScale.toArray()),this.sheenColorMap&&this.sheenColorMap.isTexture&&(a.sheenColorMap=this.sheenColorMap.toJSON(t).uuid),this.sheenRoughnessMap&&this.sheenRoughnessMap.isTexture&&(a.sheenRoughnessMap=this.sheenRoughnessMap.toJSON(t).uuid),this.dispersion!==void 0&&(a.dispersion=this.dispersion),this.iridescence!==void 0&&(a.iridescence=this.iridescence),this.iridescenceIOR!==void 0&&(a.iridescenceIOR=this.iridescenceIOR),this.iridescenceThicknessRange!==void 0&&(a.iridescenceThicknessRange=this.iridescenceThicknessRange),this.iridescenceMap&&this.iridescenceMap.isTexture&&(a.iridescenceMap=this.iridescenceMap.toJSON(t).uuid),this.iridescenceThicknessMap&&this.iridescenceThicknessMap.isTexture&&(a.iridescenceThicknessMap=this.iridescenceThicknessMap.toJSON(t).uuid),this.anisotropy!==void 0&&(a.anisotropy=this.anisotropy),this.anisotropyRotation!==void 0&&(a.anisotropyRotation=this.anisotropyRotation),this.anisotropyMap&&this.anisotropyMap.isTexture&&(a.anisotropyMap=this.anisotropyMap.toJSON(t).uuid),this.map&&this.map.isTexture&&(a.map=this.map.toJSON(t).uuid),this.matcap&&this.matcap.isTexture&&(a.matcap=this.matcap.toJSON(t).uuid),this.alphaMap&&this.alphaMap.isTexture&&(a.alphaMap=this.alphaMap.toJSON(t).uuid),this.lightMap&&this.lightMap.isTexture&&(a.lightMap=this.lightMap.toJSON(t).uuid,a.lightMapIntensity=this.lightMapIntensity),this.aoMap&&this.aoMap.isTexture&&(a.aoMap=this.aoMap.toJSON(t).uuid,a.aoMapIntensity=this.aoMapIntensity),this.bumpMap&&this.bumpMap.isTexture&&(a.bumpMap=this.bumpMap.toJSON(t).uuid,a.bumpScale=this.bumpScale),this.normalMap&&this.normalMap.isTexture&&(a.normalMap=this.normalMap.toJSON(t).uuid,a.normalMapType=this.normalMapType,a.normalScale=this.normalScale.toArray()),this.displacementMap&&this.displacementMap.isTexture&&(a.displacementMap=this.displacementMap.toJSON(t).uuid,a.displacementScale=this.displacementScale,a.displacementBias=this.displacementBias),this.roughnessMap&&this.roughnessMap.isTexture&&(a.roughnessMap=this.roughnessMap.toJSON(t).uuid),this.metalnessMap&&this.metalnessMap.isTexture&&(a.metalnessMap=this.metalnessMap.toJSON(t).uuid),this.emissiveMap&&this.emissiveMap.isTexture&&(a.emissiveMap=this.emissiveMap.toJSON(t).uuid),this.specularMap&&this.specularMap.isTexture&&(a.specularMap=this.specularMap.toJSON(t).uuid),this.specularIntensityMap&&this.specularIntensityMap.isTexture&&(a.specularIntensityMap=this.specularIntensityMap.toJSON(t).uuid),this.specularColorMap&&this.specularColorMap.isTexture&&(a.specularColorMap=this.specularColorMap.toJSON(t).uuid),this.envMap&&this.envMap.isTexture&&(a.envMap=this.envMap.toJSON(t).uuid,this.combine!==void 0&&(a.combine=this.combine)),this.envMapRotation!==void 0&&(a.envMapRotation=this.envMapRotation.toArray()),this.envMapIntensity!==void 0&&(a.envMapIntensity=this.envMapIntensity),this.reflectivity!==void 0&&(a.reflectivity=this.reflectivity),this.refractionRatio!==void 0&&(a.refractionRatio=this.refractionRatio),this.gradientMap&&this.gradientMap.isTexture&&(a.gradientMap=this.gradientMap.toJSON(t).uuid),this.transmission!==void 0&&(a.transmission=this.transmission),this.transmissionMap&&this.transmissionMap.isTexture&&(a.transmissionMap=this.transmissionMap.toJSON(t).uuid),this.thickness!==void 0&&(a.thickness=this.thickness),this.thicknessMap&&this.thicknessMap.isTexture&&(a.thicknessMap=this.thicknessMap.toJSON(t).uuid),this.attenuationDistance!==void 0&&this.attenuationDistance!==1/0&&(a.attenuationDistance=this.attenuationDistance),this.attenuationColor!==void 0&&(a.attenuationColor=this.attenuationColor.getHex()),this.size!==void 0&&(a.size=this.size),this.shadowSide!==null&&(a.shadowSide=this.shadowSide),this.sizeAttenuation!==void 0&&(a.sizeAttenuation=this.sizeAttenuation),this.blending!==io&&(a.blending=this.blending),this.side!==vs&&(a.side=this.side),this.vertexColors===!0&&(a.vertexColors=!0),this.opacity<1&&(a.opacity=this.opacity),this.transparent===!0&&(a.transparent=!0),this.blendSrc!==Ih&&(a.blendSrc=this.blendSrc),this.blendDst!==zh&&(a.blendDst=this.blendDst),this.blendEquation!==Xs&&(a.blendEquation=this.blendEquation),this.blendSrcAlpha!==null&&(a.blendSrcAlpha=this.blendSrcAlpha),this.blendDstAlpha!==null&&(a.blendDstAlpha=this.blendDstAlpha),this.blendEquationAlpha!==null&&(a.blendEquationAlpha=this.blendEquationAlpha),this.blendColor&&this.blendColor.isColor&&(a.blendColor=this.blendColor.getHex()),this.blendAlpha!==0&&(a.blendAlpha=this.blendAlpha),this.depthFunc!==so&&(a.depthFunc=this.depthFunc),this.depthTest===!1&&(a.depthTest=this.depthTest),this.depthWrite===!1&&(a.depthWrite=this.depthWrite),this.colorWrite===!1&&(a.colorWrite=this.colorWrite),this.stencilWriteMask!==255&&(a.stencilWriteMask=this.stencilWriteMask),this.stencilFunc!==J_&&(a.stencilFunc=this.stencilFunc),this.stencilRef!==0&&(a.stencilRef=this.stencilRef),this.stencilFuncMask!==255&&(a.stencilFuncMask=this.stencilFuncMask),this.stencilFail!==Or&&(a.stencilFail=this.stencilFail),this.stencilZFail!==Or&&(a.stencilZFail=this.stencilZFail),this.stencilZPass!==Or&&(a.stencilZPass=this.stencilZPass),this.stencilWrite===!0&&(a.stencilWrite=this.stencilWrite),this.rotation!==void 0&&this.rotation!==0&&(a.rotation=this.rotation),this.polygonOffset===!0&&(a.polygonOffset=!0),this.polygonOffsetFactor!==0&&(a.polygonOffsetFactor=this.polygonOffsetFactor),this.polygonOffsetUnits!==0&&(a.polygonOffsetUnits=this.polygonOffsetUnits),this.linewidth!==void 0&&this.linewidth!==1&&(a.linewidth=this.linewidth),this.dashSize!==void 0&&(a.dashSize=this.dashSize),this.gapSize!==void 0&&(a.gapSize=this.gapSize),this.scale!==void 0&&(a.scale=this.scale),this.dithering===!0&&(a.dithering=!0),this.alphaTest>0&&(a.alphaTest=this.alphaTest),this.alphaHash===!0&&(a.alphaHash=!0),this.alphaToCoverage===!0&&(a.alphaToCoverage=!0),this.premultipliedAlpha===!0&&(a.premultipliedAlpha=!0),this.forceSinglePass===!0&&(a.forceSinglePass=!0),this.allowOverride===!1&&(a.allowOverride=!1),this.wireframe===!0&&(a.wireframe=!0),this.wireframeLinewidth>1&&(a.wireframeLinewidth=this.wireframeLinewidth),this.wireframeLinecap!=="round"&&(a.wireframeLinecap=this.wireframeLinecap),this.wireframeLinejoin!=="round"&&(a.wireframeLinejoin=this.wireframeLinejoin),this.flatShading===!0&&(a.flatShading=!0),this.visible===!1&&(a.visible=!1),this.toneMapped===!1&&(a.toneMapped=!1),this.fog===!1&&(a.fog=!1),Object.keys(this.userData).length>0&&(a.userData=this.userData);function l(c){const f=[];for(const d in c){const m=c[d];delete m.metadata,f.push(m)}return f}if(n){const c=l(t.textures),f=l(t.images);c.length>0&&(a.textures=c),f.length>0&&(a.images=f)}return a}clone(){return new this.constructor().copy(this)}copy(t){this.name=t.name,this.blending=t.blending,this.side=t.side,this.vertexColors=t.vertexColors,this.opacity=t.opacity,this.transparent=t.transparent,this.blendSrc=t.blendSrc,this.blendDst=t.blendDst,this.blendEquation=t.blendEquation,this.blendSrcAlpha=t.blendSrcAlpha,this.blendDstAlpha=t.blendDstAlpha,this.blendEquationAlpha=t.blendEquationAlpha,this.blendColor.copy(t.blendColor),this.blendAlpha=t.blendAlpha,this.depthFunc=t.depthFunc,this.depthTest=t.depthTest,this.depthWrite=t.depthWrite,this.stencilWriteMask=t.stencilWriteMask,this.stencilFunc=t.stencilFunc,this.stencilRef=t.stencilRef,this.stencilFuncMask=t.stencilFuncMask,this.stencilFail=t.stencilFail,this.stencilZFail=t.stencilZFail,this.stencilZPass=t.stencilZPass,this.stencilWrite=t.stencilWrite;const n=t.clippingPlanes;let a=null;if(n!==null){const l=n.length;a=new Array(l);for(let c=0;c!==l;++c)a[c]=n[c].clone()}return this.clippingPlanes=a,this.clipIntersection=t.clipIntersection,this.clipShadows=t.clipShadows,this.shadowSide=t.shadowSide,this.colorWrite=t.colorWrite,this.precision=t.precision,this.polygonOffset=t.polygonOffset,this.polygonOffsetFactor=t.polygonOffsetFactor,this.polygonOffsetUnits=t.polygonOffsetUnits,this.dithering=t.dithering,this.alphaTest=t.alphaTest,this.alphaHash=t.alphaHash,this.alphaToCoverage=t.alphaToCoverage,this.premultipliedAlpha=t.premultipliedAlpha,this.forceSinglePass=t.forceSinglePass,this.allowOverride=t.allowOverride,this.visible=t.visible,this.toneMapped=t.toneMapped,this.userData=JSON.parse(JSON.stringify(t.userData)),this}dispose(){this.dispatchEvent({type:"dispose"})}set needsUpdate(t){t===!0&&this.version++}}class ks extends er{constructor(t){super(),this.isSpriteMaterial=!0,this.type="SpriteMaterial",this.color=new _e(16777215),this.map=null,this.alphaMap=null,this.rotation=0,this.sizeAttenuation=!0,this.transparent=!0,this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.color.copy(t.color),this.map=t.map,this.alphaMap=t.alphaMap,this.rotation=t.rotation,this.sizeAttenuation=t.sizeAttenuation,this.fog=t.fog,this}}let Xr;const cl=new k,Wr=new k,qr=new k,Yr=new ee,ul=new ee,Bx=new tn,qc=new k,fl=new k,Yc=new k,hv=new ee,_h=new ee,pv=new ee;class Zr extends kn{constructor(t=new ks){if(super(),this.isSprite=!0,this.type="Sprite",Xr===void 0){Xr=new qn;const n=new Float32Array([-.5,-.5,0,0,0,.5,-.5,0,1,0,.5,.5,0,1,1,-.5,.5,0,0,1]),a=new qb(n,5);Xr.setIndex([0,1,2,0,2,3]),Xr.setAttribute("position",new Ru(a,3,0,!1)),Xr.setAttribute("uv",new Ru(a,2,3,!1))}this.geometry=Xr,this.material=t,this.center=new ee(.5,.5),this.count=1}raycast(t,n){t.camera===null&&Ne('Sprite: "Raycaster.camera" needs to be set in order to raycast against sprites.'),Wr.setFromMatrixScale(this.matrixWorld),Bx.copy(t.camera.matrixWorld),this.modelViewMatrix.multiplyMatrices(t.camera.matrixWorldInverse,this.matrixWorld),qr.setFromMatrixPosition(this.modelViewMatrix),t.camera.isPerspectiveCamera&&this.material.sizeAttenuation===!1&&Wr.multiplyScalar(-qr.z);const a=this.material.rotation;let l,c;a!==0&&(c=Math.cos(a),l=Math.sin(a));const f=this.center;Zc(qc.set(-.5,-.5,0),qr,f,Wr,l,c),Zc(fl.set(.5,-.5,0),qr,f,Wr,l,c),Zc(Yc.set(.5,.5,0),qr,f,Wr,l,c),hv.set(0,0),_h.set(1,0),pv.set(1,1);let d=t.ray.intersectTriangle(qc,fl,Yc,!1,cl);if(d===null&&(Zc(fl.set(-.5,.5,0),qr,f,Wr,l,c),_h.set(0,1),d=t.ray.intersectTriangle(qc,Yc,fl,!1,cl),d===null))return;const m=t.ray.origin.distanceTo(cl);m<t.near||m>t.far||n.push({distance:m,point:cl.clone(),uv:Ri.getInterpolation(cl,qc,fl,Yc,hv,_h,pv,new ee),face:null,object:this})}copy(t,n){return super.copy(t,n),t.center!==void 0&&this.center.copy(t.center),this.material=t.material,this}}function Zc(r,t,n,a,l,c){Yr.subVectors(r,n).addScalar(.5).multiply(a),l!==void 0?(ul.x=c*Yr.x-l*Yr.y,ul.y=l*Yr.x+c*Yr.y):ul.copy(Yr),r.copy(t),r.x+=ul.x,r.y+=ul.y,r.applyMatrix4(Bx)}const Ta=new k,vh=new k,Kc=new k,hs=new k,xh=new k,Qc=new k,yh=new k;class Yp{constructor(t=new k,n=new k(0,0,-1)){this.origin=t,this.direction=n}set(t,n){return this.origin.copy(t),this.direction.copy(n),this}copy(t){return this.origin.copy(t.origin),this.direction.copy(t.direction),this}at(t,n){return n.copy(this.origin).addScaledVector(this.direction,t)}lookAt(t){return this.direction.copy(t).sub(this.origin).normalize(),this}recast(t){return this.origin.copy(this.at(t,Ta)),this}closestPointToPoint(t,n){n.subVectors(t,this.origin);const a=n.dot(this.direction);return a<0?n.copy(this.origin):n.copy(this.origin).addScaledVector(this.direction,a)}distanceToPoint(t){return Math.sqrt(this.distanceSqToPoint(t))}distanceSqToPoint(t){const n=Ta.subVectors(t,this.origin).dot(this.direction);return n<0?this.origin.distanceToSquared(t):(Ta.copy(this.origin).addScaledVector(this.direction,n),Ta.distanceToSquared(t))}distanceSqToSegment(t,n,a,l){vh.copy(t).add(n).multiplyScalar(.5),Kc.copy(n).sub(t).normalize(),hs.copy(this.origin).sub(vh);const c=t.distanceTo(n)*.5,f=-this.direction.dot(Kc),d=hs.dot(this.direction),m=-hs.dot(Kc),h=hs.lengthSq(),g=Math.abs(1-f*f);let _,v,y,E;if(g>0)if(_=f*m-d,v=f*d-m,E=c*g,_>=0)if(v>=-E)if(v<=E){const A=1/g;_*=A,v*=A,y=_*(_+f*v+2*d)+v*(f*_+v+2*m)+h}else v=c,_=Math.max(0,-(f*v+d)),y=-_*_+v*(v+2*m)+h;else v=-c,_=Math.max(0,-(f*v+d)),y=-_*_+v*(v+2*m)+h;else v<=-E?(_=Math.max(0,-(-f*c+d)),v=_>0?-c:Math.min(Math.max(-c,-m),c),y=-_*_+v*(v+2*m)+h):v<=E?(_=0,v=Math.min(Math.max(-c,-m),c),y=v*(v+2*m)+h):(_=Math.max(0,-(f*c+d)),v=_>0?c:Math.min(Math.max(-c,-m),c),y=-_*_+v*(v+2*m)+h);else v=f>0?-c:c,_=Math.max(0,-(f*v+d)),y=-_*_+v*(v+2*m)+h;return a&&a.copy(this.origin).addScaledVector(this.direction,_),l&&l.copy(vh).addScaledVector(Kc,v),y}intersectSphere(t,n){Ta.subVectors(t.center,this.origin);const a=Ta.dot(this.direction),l=Ta.dot(Ta)-a*a,c=t.radius*t.radius;if(l>c)return null;const f=Math.sqrt(c-l),d=a-f,m=a+f;return m<0?null:d<0?this.at(m,n):this.at(d,n)}intersectsSphere(t){return t.radius<0?!1:this.distanceSqToPoint(t.center)<=t.radius*t.radius}distanceToPlane(t){const n=t.normal.dot(this.direction);if(n===0)return t.distanceToPoint(this.origin)===0?0:null;const a=-(this.origin.dot(t.normal)+t.constant)/n;return a>=0?a:null}intersectPlane(t,n){const a=this.distanceToPlane(t);return a===null?null:this.at(a,n)}intersectsPlane(t){const n=t.distanceToPoint(this.origin);return n===0||t.normal.dot(this.direction)*n<0}intersectBox(t,n){let a,l,c,f,d,m;const h=1/this.direction.x,g=1/this.direction.y,_=1/this.direction.z,v=this.origin;return h>=0?(a=(t.min.x-v.x)*h,l=(t.max.x-v.x)*h):(a=(t.max.x-v.x)*h,l=(t.min.x-v.x)*h),g>=0?(c=(t.min.y-v.y)*g,f=(t.max.y-v.y)*g):(c=(t.max.y-v.y)*g,f=(t.min.y-v.y)*g),a>f||c>l||((c>a||isNaN(a))&&(a=c),(f<l||isNaN(l))&&(l=f),_>=0?(d=(t.min.z-v.z)*_,m=(t.max.z-v.z)*_):(d=(t.max.z-v.z)*_,m=(t.min.z-v.z)*_),a>m||d>l)||((d>a||a!==a)&&(a=d),(m<l||l!==l)&&(l=m),l<0)?null:this.at(a>=0?a:l,n)}intersectsBox(t){return this.intersectBox(t,Ta)!==null}intersectTriangle(t,n,a,l,c){xh.subVectors(n,t),Qc.subVectors(a,t),yh.crossVectors(xh,Qc);let f=this.direction.dot(yh),d;if(f>0){if(l)return null;d=1}else if(f<0)d=-1,f=-f;else return null;hs.subVectors(this.origin,t);const m=d*this.direction.dot(Qc.crossVectors(hs,Qc));if(m<0)return null;const h=d*this.direction.dot(xh.cross(hs));if(h<0||m+h>f)return null;const g=-d*hs.dot(yh);return g<0?null:this.at(g/f,c)}applyMatrix4(t){return this.origin.applyMatrix4(t),this.direction.transformDirection(t),this}equals(t){return t.origin.equals(this.origin)&&t.direction.equals(this.direction)}clone(){return new this.constructor().copy(this)}}class Ws extends er{constructor(t){super(),this.isMeshBasicMaterial=!0,this.type="MeshBasicMaterial",this.color=new _e(16777215),this.map=null,this.lightMap=null,this.lightMapIntensity=1,this.aoMap=null,this.aoMapIntensity=1,this.specularMap=null,this.alphaMap=null,this.envMap=null,this.envMapRotation=new xs,this.combine=_x,this.reflectivity=1,this.refractionRatio=.98,this.wireframe=!1,this.wireframeLinewidth=1,this.wireframeLinecap="round",this.wireframeLinejoin="round",this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.color.copy(t.color),this.map=t.map,this.lightMap=t.lightMap,this.lightMapIntensity=t.lightMapIntensity,this.aoMap=t.aoMap,this.aoMapIntensity=t.aoMapIntensity,this.specularMap=t.specularMap,this.alphaMap=t.alphaMap,this.envMap=t.envMap,this.envMapRotation.copy(t.envMapRotation),this.combine=t.combine,this.reflectivity=t.reflectivity,this.refractionRatio=t.refractionRatio,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.wireframeLinecap=t.wireframeLinecap,this.wireframeLinejoin=t.wireframeLinejoin,this.fog=t.fog,this}}const mv=new tn,Gs=new Yp,Jc=new Ou,gv=new k,$c=new k,tu=new k,eu=new k,Sh=new k,nu=new k,_v=new k,iu=new k;class Gn extends kn{constructor(t=new qn,n=new Ws){super(),this.isMesh=!0,this.type="Mesh",this.geometry=t,this.material=n,this.morphTargetDictionary=void 0,this.morphTargetInfluences=void 0,this.count=1,this.updateMorphTargets()}copy(t,n){return super.copy(t,n),t.morphTargetInfluences!==void 0&&(this.morphTargetInfluences=t.morphTargetInfluences.slice()),t.morphTargetDictionary!==void 0&&(this.morphTargetDictionary=Object.assign({},t.morphTargetDictionary)),this.material=Array.isArray(t.material)?t.material.slice():t.material,this.geometry=t.geometry,this}updateMorphTargets(){const n=this.geometry.morphAttributes,a=Object.keys(n);if(a.length>0){const l=n[a[0]];if(l!==void 0){this.morphTargetInfluences=[],this.morphTargetDictionary={};for(let c=0,f=l.length;c<f;c++){const d=l[c].name||String(c);this.morphTargetInfluences.push(0),this.morphTargetDictionary[d]=c}}}}getVertexPosition(t,n){const a=this.geometry,l=a.attributes.position,c=a.morphAttributes.position,f=a.morphTargetsRelative;n.fromBufferAttribute(l,t);const d=this.morphTargetInfluences;if(c&&d){nu.set(0,0,0);for(let m=0,h=c.length;m<h;m++){const g=d[m],_=c[m];g!==0&&(Sh.fromBufferAttribute(_,t),f?nu.addScaledVector(Sh,g):nu.addScaledVector(Sh.sub(n),g))}n.add(nu)}return n}raycast(t,n){const a=this.geometry,l=this.material,c=this.matrixWorld;l!==void 0&&(a.boundingSphere===null&&a.computeBoundingSphere(),Jc.copy(a.boundingSphere),Jc.applyMatrix4(c),Gs.copy(t.ray).recast(t.near),!(Jc.containsPoint(Gs.origin)===!1&&(Gs.intersectSphere(Jc,gv)===null||Gs.origin.distanceToSquared(gv)>(t.far-t.near)**2))&&(mv.copy(c).invert(),Gs.copy(t.ray).applyMatrix4(mv),!(a.boundingBox!==null&&Gs.intersectsBox(a.boundingBox)===!1)&&this._computeIntersections(t,n,Gs)))}_computeIntersections(t,n,a){let l;const c=this.geometry,f=this.material,d=c.index,m=c.attributes.position,h=c.attributes.uv,g=c.attributes.uv1,_=c.attributes.normal,v=c.groups,y=c.drawRange;if(d!==null)if(Array.isArray(f))for(let E=0,A=v.length;E<A;E++){const S=v[E],x=f[S.materialIndex],w=Math.max(S.start,y.start),D=Math.min(d.count,Math.min(S.start+S.count,y.start+y.count));for(let U=w,G=D;U<G;U+=3){const O=d.getX(U),B=d.getX(U+1),R=d.getX(U+2);l=au(this,x,t,a,h,g,_,O,B,R),l&&(l.faceIndex=Math.floor(U/3),l.face.materialIndex=S.materialIndex,n.push(l))}}else{const E=Math.max(0,y.start),A=Math.min(d.count,y.start+y.count);for(let S=E,x=A;S<x;S+=3){const w=d.getX(S),D=d.getX(S+1),U=d.getX(S+2);l=au(this,f,t,a,h,g,_,w,D,U),l&&(l.faceIndex=Math.floor(S/3),n.push(l))}}else if(m!==void 0)if(Array.isArray(f))for(let E=0,A=v.length;E<A;E++){const S=v[E],x=f[S.materialIndex],w=Math.max(S.start,y.start),D=Math.min(m.count,Math.min(S.start+S.count,y.start+y.count));for(let U=w,G=D;U<G;U+=3){const O=U,B=U+1,R=U+2;l=au(this,x,t,a,h,g,_,O,B,R),l&&(l.faceIndex=Math.floor(U/3),l.face.materialIndex=S.materialIndex,n.push(l))}}else{const E=Math.max(0,y.start),A=Math.min(m.count,y.start+y.count);for(let S=E,x=A;S<x;S+=3){const w=S,D=S+1,U=S+2;l=au(this,f,t,a,h,g,_,w,D,U),l&&(l.faceIndex=Math.floor(S/3),n.push(l))}}}}function Zb(r,t,n,a,l,c,f,d){let m;if(t.side===ti?m=a.intersectTriangle(f,c,l,!0,d):m=a.intersectTriangle(l,c,f,t.side===vs,d),m===null)return null;iu.copy(d),iu.applyMatrix4(r.matrixWorld);const h=n.ray.origin.distanceTo(iu);return h<n.near||h>n.far?null:{distance:h,point:iu.clone(),object:r}}function au(r,t,n,a,l,c,f,d,m,h){r.getVertexPosition(d,$c),r.getVertexPosition(m,tu),r.getVertexPosition(h,eu);const g=Zb(r,t,n,a,$c,tu,eu,_v);if(g){const _=new k;Ri.getBarycoord(_v,$c,tu,eu,_),l&&(g.uv=Ri.getInterpolatedAttribute(l,d,m,h,_,new ee)),c&&(g.uv1=Ri.getInterpolatedAttribute(c,d,m,h,_,new ee)),f&&(g.normal=Ri.getInterpolatedAttribute(f,d,m,h,_,new k),g.normal.dot(a.direction)>0&&g.normal.multiplyScalar(-1));const v={a:d,b:m,c:h,normal:new k,materialIndex:0};Ri.getNormal($c,tu,eu,v.normal),g.face=v,g.barycoord=_}return g}class Kb extends Vn{constructor(t=null,n=1,a=1,l,c,f,d,m,h=Pn,g=Pn,_,v){super(null,f,d,m,h,g,l,c,_,v),this.isDataTexture=!0,this.image={data:t,width:n,height:a},this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1}}const Mh=new k,Qb=new k,Jb=new pe;class js{constructor(t=new k(1,0,0),n=0){this.isPlane=!0,this.normal=t,this.constant=n}set(t,n){return this.normal.copy(t),this.constant=n,this}setComponents(t,n,a,l){return this.normal.set(t,n,a),this.constant=l,this}setFromNormalAndCoplanarPoint(t,n){return this.normal.copy(t),this.constant=-n.dot(this.normal),this}setFromCoplanarPoints(t,n,a){const l=Mh.subVectors(a,n).cross(Qb.subVectors(t,n)).normalize();return this.setFromNormalAndCoplanarPoint(l,t),this}copy(t){return this.normal.copy(t.normal),this.constant=t.constant,this}normalize(){const t=1/this.normal.length();return this.normal.multiplyScalar(t),this.constant*=t,this}negate(){return this.constant*=-1,this.normal.negate(),this}distanceToPoint(t){return this.normal.dot(t)+this.constant}distanceToSphere(t){return this.distanceToPoint(t.center)-t.radius}projectPoint(t,n){return n.copy(t).addScaledVector(this.normal,-this.distanceToPoint(t))}intersectLine(t,n,a=!0){const l=t.delta(Mh),c=this.normal.dot(l);if(c===0)return this.distanceToPoint(t.start)===0?n.copy(t.start):null;const f=-(t.start.dot(this.normal)+this.constant)/c;return a===!0&&(f<0||f>1)?null:n.copy(t.start).addScaledVector(l,f)}intersectsLine(t){const n=this.distanceToPoint(t.start),a=this.distanceToPoint(t.end);return n<0&&a>0||a<0&&n>0}intersectsBox(t){return t.intersectsPlane(this)}intersectsSphere(t){return t.intersectsPlane(this)}coplanarPoint(t){return t.copy(this.normal).multiplyScalar(-this.constant)}applyMatrix4(t,n){const a=n||Jb.getNormalMatrix(t),l=this.coplanarPoint(Mh).applyMatrix4(t),c=this.normal.applyMatrix3(a).normalize();return this.constant=-l.dot(c),this}translate(t){return this.constant-=t.dot(this.normal),this}equals(t){return t.normal.equals(this.normal)&&t.constant===this.constant}clone(){return new this.constructor().copy(this)}}const Vs=new Ou,$b=new ee(.5,.5),su=new k;class Zp{constructor(t=new js,n=new js,a=new js,l=new js,c=new js,f=new js){this.planes=[t,n,a,l,c,f]}set(t,n,a,l,c,f){const d=this.planes;return d[0].copy(t),d[1].copy(n),d[2].copy(a),d[3].copy(l),d[4].copy(c),d[5].copy(f),this}copy(t){const n=this.planes;for(let a=0;a<6;a++)n[a].copy(t.planes[a]);return this}setFromProjectionMatrix(t,n=Ji,a=!1){const l=this.planes,c=t.elements,f=c[0],d=c[1],m=c[2],h=c[3],g=c[4],_=c[5],v=c[6],y=c[7],E=c[8],A=c[9],S=c[10],x=c[11],w=c[12],D=c[13],U=c[14],G=c[15];if(l[0].setComponents(h-f,y-g,x-E,G-w).normalize(),l[1].setComponents(h+f,y+g,x+E,G+w).normalize(),l[2].setComponents(h+d,y+_,x+A,G+D).normalize(),l[3].setComponents(h-d,y-_,x-A,G-D).normalize(),a)l[4].setComponents(m,v,S,U).normalize(),l[5].setComponents(h-m,y-v,x-S,G-U).normalize();else if(l[4].setComponents(h-m,y-v,x-S,G-U).normalize(),n===Ji)l[5].setComponents(h+m,y+v,x+S,G+U).normalize();else if(n===El)l[5].setComponents(m,v,S,U).normalize();else throw new Error("THREE.Frustum.setFromProjectionMatrix(): Invalid coordinate system: "+n);return this}intersectsObject(t){if(t.boundingSphere!==void 0)t.boundingSphere===null&&t.computeBoundingSphere(),Vs.copy(t.boundingSphere).applyMatrix4(t.matrixWorld);else{const n=t.geometry;n.boundingSphere===null&&n.computeBoundingSphere(),Vs.copy(n.boundingSphere).applyMatrix4(t.matrixWorld)}return this.intersectsSphere(Vs)}intersectsSprite(t){Vs.center.set(0,0,0);const n=$b.distanceTo(t.center);return Vs.radius=.7071067811865476+n,Vs.applyMatrix4(t.matrixWorld),this.intersectsSphere(Vs)}intersectsSphere(t){const n=this.planes,a=t.center,l=-t.radius;for(let c=0;c<6;c++)if(n[c].distanceToPoint(a)<l)return!1;return!0}intersectsBox(t){const n=this.planes;for(let a=0;a<6;a++){const l=n[a];if(su.x=l.normal.x>0?t.max.x:t.min.x,su.y=l.normal.y>0?t.max.y:t.min.y,su.z=l.normal.z>0?t.max.z:t.min.z,l.distanceToPoint(su)<0)return!1}return!0}containsPoint(t){const n=this.planes;for(let a=0;a<6;a++)if(n[a].distanceToPoint(t)<0)return!1;return!0}clone(){return new this.constructor().copy(this)}}class Fx extends er{constructor(t){super(),this.isPointsMaterial=!0,this.type="PointsMaterial",this.color=new _e(16777215),this.map=null,this.alphaMap=null,this.size=1,this.sizeAttenuation=!0,this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.color.copy(t.color),this.map=t.map,this.alphaMap=t.alphaMap,this.size=t.size,this.sizeAttenuation=t.sizeAttenuation,this.fog=t.fog,this}}const vv=new tn,Ap=new Yp,ru=new Ou,ou=new k;class tE extends kn{constructor(t=new qn,n=new Fx){super(),this.isPoints=!0,this.type="Points",this.geometry=t,this.material=n,this.morphTargetDictionary=void 0,this.morphTargetInfluences=void 0,this.updateMorphTargets()}copy(t,n){return super.copy(t,n),this.material=Array.isArray(t.material)?t.material.slice():t.material,this.geometry=t.geometry,this}raycast(t,n){const a=this.geometry,l=this.matrixWorld,c=t.params.Points.threshold,f=a.drawRange;if(a.boundingSphere===null&&a.computeBoundingSphere(),ru.copy(a.boundingSphere),ru.applyMatrix4(l),ru.radius+=c,t.ray.intersectsSphere(ru)===!1)return;vv.copy(l).invert(),Ap.copy(t.ray).applyMatrix4(vv);const d=c/((this.scale.x+this.scale.y+this.scale.z)/3),m=d*d,h=a.index,_=a.attributes.position;if(h!==null){const v=Math.max(0,f.start),y=Math.min(h.count,f.start+f.count);for(let E=v,A=y;E<A;E++){const S=h.getX(E);ou.fromBufferAttribute(_,S),xv(ou,S,m,l,t,n,this)}}else{const v=Math.max(0,f.start),y=Math.min(_.count,f.start+f.count);for(let E=v,A=y;E<A;E++)ou.fromBufferAttribute(_,E),xv(ou,E,m,l,t,n,this)}}updateMorphTargets(){const n=this.geometry.morphAttributes,a=Object.keys(n);if(a.length>0){const l=n[a[0]];if(l!==void 0){this.morphTargetInfluences=[],this.morphTargetDictionary={};for(let c=0,f=l.length;c<f;c++){const d=l[c].name||String(c);this.morphTargetInfluences.push(0),this.morphTargetDictionary[d]=c}}}}}function xv(r,t,n,a,l,c,f){const d=Ap.distanceSqToPoint(r);if(d<n){const m=new k;Ap.closestPointToPoint(r,m),m.applyMatrix4(a);const h=l.ray.origin.distanceTo(m);if(h<l.near||h>l.far)return;c.push({distance:h,distanceToRay:Math.sqrt(d),point:m,index:t,face:null,faceIndex:null,barycoord:null,object:f})}}class Hx extends Vn{constructor(t=[],n=Js,a,l,c,f,d,m,h,g){super(t,n,a,l,c,f,d,m,h,g),this.isCubeTexture=!0,this.flipY=!1}get images(){return this.image}set images(t){this.image=t}}class qs extends Vn{constructor(t,n,a,l,c,f,d,m,h){super(t,n,a,l,c,f,d,m,h),this.isCanvasTexture=!0,this.needsUpdate=!0}}class oo extends Vn{constructor(t,n,a=ea,l,c,f,d=Pn,m=Pn,h,g=Ua,_=1){if(g!==Ua&&g!==Ks)throw new Error("DepthTexture format must be either THREE.DepthFormat or THREE.DepthStencilFormat");const v={width:t,height:n,depth:_};super(v,l,c,f,d,m,g,a,h),this.isDepthTexture=!0,this.flipY=!1,this.generateMipmaps=!1,this.compareFunction=null}copy(t){return super.copy(t),this.source=new Wp(Object.assign({},t.image)),this.compareFunction=t.compareFunction,this}toJSON(t){const n=super.toJSON(t);return this.compareFunction!==null&&(n.compareFunction=this.compareFunction),n}}class eE extends oo{constructor(t,n=ea,a=Js,l,c,f=Pn,d=Pn,m,h=Ua){const g={width:t,height:t,depth:1},_=[g,g,g,g,g,g];super(t,t,n,a,l,c,f,d,m,h),this.image=_,this.isCubeDepthTexture=!0,this.isCubeTexture=!0}get images(){return this.image}set images(t){this.image=t}}class Gx extends Vn{constructor(t=null){super(),this.sourceTexture=t,this.isExternalTexture=!0}copy(t){return super.copy(t),this.sourceTexture=t.sourceTexture,this}}class Rl extends qn{constructor(t=1,n=1,a=1,l=1,c=1,f=1){super(),this.type="BoxGeometry",this.parameters={width:t,height:n,depth:a,widthSegments:l,heightSegments:c,depthSegments:f};const d=this;l=Math.floor(l),c=Math.floor(c),f=Math.floor(f);const m=[],h=[],g=[],_=[];let v=0,y=0;E("z","y","x",-1,-1,a,n,t,f,c,0),E("z","y","x",1,-1,a,n,-t,f,c,1),E("x","z","y",1,1,t,a,n,l,f,2),E("x","z","y",1,-1,t,a,-n,l,f,3),E("x","y","z",1,-1,t,n,a,l,c,4),E("x","y","z",-1,-1,t,n,-a,l,c,5),this.setIndex(m),this.setAttribute("position",new Rn(h,3)),this.setAttribute("normal",new Rn(g,3)),this.setAttribute("uv",new Rn(_,2));function E(A,S,x,w,D,U,G,O,B,R,z){const K=U/B,V=G/R,$=U/2,ht=G/2,gt=O/2,q=B+1,P=R+1;let F=0,ct=0;const J=new k;for(let xt=0;xt<P;xt++){const I=xt*V-ht;for(let Q=0;Q<q;Q++){const Mt=Q*K-$;J[A]=Mt*w,J[S]=I*D,J[x]=gt,h.push(J.x,J.y,J.z),J[A]=0,J[S]=0,J[x]=O>0?1:-1,g.push(J.x,J.y,J.z),_.push(Q/B),_.push(1-xt/R),F+=1}}for(let xt=0;xt<R;xt++)for(let I=0;I<B;I++){const Q=v+I+q*xt,Mt=v+I+q*(xt+1),Rt=v+(I+1)+q*(xt+1),wt=v+(I+1)+q*xt;m.push(Q,Mt,wt),m.push(Mt,Rt,wt),ct+=6}d.addGroup(y,ct,z),y+=ct,v+=F}}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new Rl(t.width,t.height,t.depth,t.widthSegments,t.heightSegments,t.depthSegments)}}class La{constructor(){this.type="Curve",this.arcLengthDivisions=200,this.needsUpdate=!1,this.cacheArcLengths=null}getPoint(){ce("Curve: .getPoint() not implemented.")}getPointAt(t,n){const a=this.getUtoTmapping(t);return this.getPoint(a,n)}getPoints(t=5){const n=[];for(let a=0;a<=t;a++)n.push(this.getPoint(a/t));return n}getSpacedPoints(t=5){const n=[];for(let a=0;a<=t;a++)n.push(this.getPointAt(a/t));return n}getLength(){const t=this.getLengths();return t[t.length-1]}getLengths(t=this.arcLengthDivisions){if(this.cacheArcLengths&&this.cacheArcLengths.length===t+1&&!this.needsUpdate)return this.cacheArcLengths;this.needsUpdate=!1;const n=[];let a,l=this.getPoint(0),c=0;n.push(0);for(let f=1;f<=t;f++)a=this.getPoint(f/t),c+=a.distanceTo(l),n.push(c),l=a;return this.cacheArcLengths=n,n}updateArcLengths(){this.needsUpdate=!0,this.getLengths()}getUtoTmapping(t,n=null){const a=this.getLengths();let l=0;const c=a.length;let f;n?f=n:f=t*a[c-1];let d=0,m=c-1,h;for(;d<=m;)if(l=Math.floor(d+(m-d)/2),h=a[l]-f,h<0)d=l+1;else if(h>0)m=l-1;else{m=l;break}if(l=m,a[l]===f)return l/(c-1);const g=a[l],v=a[l+1]-g,y=(f-g)/v;return(l+y)/(c-1)}getTangent(t,n){let l=t-1e-4,c=t+1e-4;l<0&&(l=0),c>1&&(c=1);const f=this.getPoint(l),d=this.getPoint(c),m=n||(f.isVector2?new ee:new k);return m.copy(d).sub(f).normalize(),m}getTangentAt(t,n){const a=this.getUtoTmapping(t);return this.getTangent(a,n)}computeFrenetFrames(t,n=!1){const a=new k,l=[],c=[],f=[],d=new k,m=new tn;for(let y=0;y<=t;y++){const E=y/t;l[y]=this.getTangentAt(E,new k)}c[0]=new k,f[0]=new k;let h=Number.MAX_VALUE;const g=Math.abs(l[0].x),_=Math.abs(l[0].y),v=Math.abs(l[0].z);g<=h&&(h=g,a.set(1,0,0)),_<=h&&(h=_,a.set(0,1,0)),v<=h&&a.set(0,0,1),d.crossVectors(l[0],a).normalize(),c[0].crossVectors(l[0],d),f[0].crossVectors(l[0],c[0]);for(let y=1;y<=t;y++){if(c[y]=c[y-1].clone(),f[y]=f[y-1].clone(),d.crossVectors(l[y-1],l[y]),d.length()>Number.EPSILON){d.normalize();const E=Math.acos(Se(l[y-1].dot(l[y]),-1,1));c[y].applyMatrix4(m.makeRotationAxis(d,E))}f[y].crossVectors(l[y],c[y])}if(n===!0){let y=Math.acos(Se(c[0].dot(c[t]),-1,1));y/=t,l[0].dot(d.crossVectors(c[0],c[t]))>0&&(y=-y);for(let E=1;E<=t;E++)c[E].applyMatrix4(m.makeRotationAxis(l[E],y*E)),f[E].crossVectors(l[E],c[E])}return{tangents:l,normals:c,binormals:f}}clone(){return new this.constructor().copy(this)}copy(t){return this.arcLengthDivisions=t.arcLengthDivisions,this}toJSON(){const t={metadata:{version:4.7,type:"Curve",generator:"Curve.toJSON"}};return t.arcLengthDivisions=this.arcLengthDivisions,t.type=this.type,t}fromJSON(t){return this.arcLengthDivisions=t.arcLengthDivisions,this}}class Vx extends La{constructor(t=0,n=0,a=1,l=1,c=0,f=Math.PI*2,d=!1,m=0){super(),this.isEllipseCurve=!0,this.type="EllipseCurve",this.aX=t,this.aY=n,this.xRadius=a,this.yRadius=l,this.aStartAngle=c,this.aEndAngle=f,this.aClockwise=d,this.aRotation=m}getPoint(t,n=new ee){const a=n,l=Math.PI*2;let c=this.aEndAngle-this.aStartAngle;const f=Math.abs(c)<Number.EPSILON;for(;c<0;)c+=l;for(;c>l;)c-=l;c<Number.EPSILON&&(f?c=0:c=l),this.aClockwise===!0&&!f&&(c===l?c=-l:c=c-l);const d=this.aStartAngle+t*c;let m=this.aX+this.xRadius*Math.cos(d),h=this.aY+this.yRadius*Math.sin(d);if(this.aRotation!==0){const g=Math.cos(this.aRotation),_=Math.sin(this.aRotation),v=m-this.aX,y=h-this.aY;m=v*g-y*_+this.aX,h=v*_+y*g+this.aY}return a.set(m,h)}copy(t){return super.copy(t),this.aX=t.aX,this.aY=t.aY,this.xRadius=t.xRadius,this.yRadius=t.yRadius,this.aStartAngle=t.aStartAngle,this.aEndAngle=t.aEndAngle,this.aClockwise=t.aClockwise,this.aRotation=t.aRotation,this}toJSON(){const t=super.toJSON();return t.aX=this.aX,t.aY=this.aY,t.xRadius=this.xRadius,t.yRadius=this.yRadius,t.aStartAngle=this.aStartAngle,t.aEndAngle=this.aEndAngle,t.aClockwise=this.aClockwise,t.aRotation=this.aRotation,t}fromJSON(t){return super.fromJSON(t),this.aX=t.aX,this.aY=t.aY,this.xRadius=t.xRadius,this.yRadius=t.yRadius,this.aStartAngle=t.aStartAngle,this.aEndAngle=t.aEndAngle,this.aClockwise=t.aClockwise,this.aRotation=t.aRotation,this}}class nE extends Vx{constructor(t,n,a,l,c,f){super(t,n,a,a,l,c,f),this.isArcCurve=!0,this.type="ArcCurve"}}function Kp(){let r=0,t=0,n=0,a=0;function l(c,f,d,m){r=c,t=d,n=-3*c+3*f-2*d-m,a=2*c-2*f+d+m}return{initCatmullRom:function(c,f,d,m,h){l(f,d,h*(d-c),h*(m-f))},initNonuniformCatmullRom:function(c,f,d,m,h,g,_){let v=(f-c)/h-(d-c)/(h+g)+(d-f)/g,y=(d-f)/g-(m-f)/(g+_)+(m-d)/_;v*=g,y*=g,l(f,d,v,y)},calc:function(c){const f=c*c,d=f*c;return r+t*c+n*f+a*d}}}const yv=new k,Sv=new k,bh=new Kp,Eh=new Kp,Th=new Kp;class kx extends La{constructor(t=[],n=!1,a="centripetal",l=.5){super(),this.isCatmullRomCurve3=!0,this.type="CatmullRomCurve3",this.points=t,this.closed=n,this.curveType=a,this.tension=l}getPoint(t,n=new k){const a=n,l=this.points,c=l.length,f=(c-(this.closed?0:1))*t;let d=Math.floor(f),m=f-d;this.closed?d+=d>0?0:(Math.floor(Math.abs(d)/c)+1)*c:m===0&&d===c-1&&(d=c-2,m=1);let h,g;this.closed||d>0?h=l[(d-1)%c]:(Sv.subVectors(l[0],l[1]).add(l[0]),h=Sv);const _=l[d%c],v=l[(d+1)%c];if(this.closed||d+2<c?g=l[(d+2)%c]:(yv.subVectors(l[c-1],l[c-2]).add(l[c-1]),g=yv),this.curveType==="centripetal"||this.curveType==="chordal"){const y=this.curveType==="chordal"?.5:.25;let E=Math.pow(h.distanceToSquared(_),y),A=Math.pow(_.distanceToSquared(v),y),S=Math.pow(v.distanceToSquared(g),y);A<1e-4&&(A=1),E<1e-4&&(E=A),S<1e-4&&(S=A),bh.initNonuniformCatmullRom(h.x,_.x,v.x,g.x,E,A,S),Eh.initNonuniformCatmullRom(h.y,_.y,v.y,g.y,E,A,S),Th.initNonuniformCatmullRom(h.z,_.z,v.z,g.z,E,A,S)}else this.curveType==="catmullrom"&&(bh.initCatmullRom(h.x,_.x,v.x,g.x,this.tension),Eh.initCatmullRom(h.y,_.y,v.y,g.y,this.tension),Th.initCatmullRom(h.z,_.z,v.z,g.z,this.tension));return a.set(bh.calc(m),Eh.calc(m),Th.calc(m)),a}copy(t){super.copy(t),this.points=[];for(let n=0,a=t.points.length;n<a;n++){const l=t.points[n];this.points.push(l.clone())}return this.closed=t.closed,this.curveType=t.curveType,this.tension=t.tension,this}toJSON(){const t=super.toJSON();t.points=[];for(let n=0,a=this.points.length;n<a;n++){const l=this.points[n];t.points.push(l.toArray())}return t.closed=this.closed,t.curveType=this.curveType,t.tension=this.tension,t}fromJSON(t){super.fromJSON(t),this.points=[];for(let n=0,a=t.points.length;n<a;n++){const l=t.points[n];this.points.push(new k().fromArray(l))}return this.closed=t.closed,this.curveType=t.curveType,this.tension=t.tension,this}}function Mv(r,t,n,a,l){const c=(a-t)*.5,f=(l-n)*.5,d=r*r,m=r*d;return(2*n-2*a+c+f)*m+(-3*n+3*a-2*c-f)*d+c*r+n}function iE(r,t){const n=1-r;return n*n*t}function aE(r,t){return 2*(1-r)*r*t}function sE(r,t){return r*r*t}function yl(r,t,n,a){return iE(r,t)+aE(r,n)+sE(r,a)}function rE(r,t){const n=1-r;return n*n*n*t}function oE(r,t){const n=1-r;return 3*n*n*r*t}function lE(r,t){return 3*(1-r)*r*r*t}function cE(r,t){return r*r*r*t}function Sl(r,t,n,a,l){return rE(r,t)+oE(r,n)+lE(r,a)+cE(r,l)}class uE extends La{constructor(t=new ee,n=new ee,a=new ee,l=new ee){super(),this.isCubicBezierCurve=!0,this.type="CubicBezierCurve",this.v0=t,this.v1=n,this.v2=a,this.v3=l}getPoint(t,n=new ee){const a=n,l=this.v0,c=this.v1,f=this.v2,d=this.v3;return a.set(Sl(t,l.x,c.x,f.x,d.x),Sl(t,l.y,c.y,f.y,d.y)),a}copy(t){return super.copy(t),this.v0.copy(t.v0),this.v1.copy(t.v1),this.v2.copy(t.v2),this.v3.copy(t.v3),this}toJSON(){const t=super.toJSON();return t.v0=this.v0.toArray(),t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t.v3=this.v3.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v0.fromArray(t.v0),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this.v3.fromArray(t.v3),this}}class fE extends La{constructor(t=new k,n=new k,a=new k,l=new k){super(),this.isCubicBezierCurve3=!0,this.type="CubicBezierCurve3",this.v0=t,this.v1=n,this.v2=a,this.v3=l}getPoint(t,n=new k){const a=n,l=this.v0,c=this.v1,f=this.v2,d=this.v3;return a.set(Sl(t,l.x,c.x,f.x,d.x),Sl(t,l.y,c.y,f.y,d.y),Sl(t,l.z,c.z,f.z,d.z)),a}copy(t){return super.copy(t),this.v0.copy(t.v0),this.v1.copy(t.v1),this.v2.copy(t.v2),this.v3.copy(t.v3),this}toJSON(){const t=super.toJSON();return t.v0=this.v0.toArray(),t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t.v3=this.v3.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v0.fromArray(t.v0),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this.v3.fromArray(t.v3),this}}class dE extends La{constructor(t=new ee,n=new ee){super(),this.isLineCurve=!0,this.type="LineCurve",this.v1=t,this.v2=n}getPoint(t,n=new ee){const a=n;return t===1?a.copy(this.v2):(a.copy(this.v2).sub(this.v1),a.multiplyScalar(t).add(this.v1)),a}getPointAt(t,n){return this.getPoint(t,n)}getTangent(t,n=new ee){return n.subVectors(this.v2,this.v1).normalize()}getTangentAt(t,n){return this.getTangent(t,n)}copy(t){return super.copy(t),this.v1.copy(t.v1),this.v2.copy(t.v2),this}toJSON(){const t=super.toJSON();return t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this}}class hE extends La{constructor(t=new k,n=new k){super(),this.isLineCurve3=!0,this.type="LineCurve3",this.v1=t,this.v2=n}getPoint(t,n=new k){const a=n;return t===1?a.copy(this.v2):(a.copy(this.v2).sub(this.v1),a.multiplyScalar(t).add(this.v1)),a}getPointAt(t,n){return this.getPoint(t,n)}getTangent(t,n=new k){return n.subVectors(this.v2,this.v1).normalize()}getTangentAt(t,n){return this.getTangent(t,n)}copy(t){return super.copy(t),this.v1.copy(t.v1),this.v2.copy(t.v2),this}toJSON(){const t=super.toJSON();return t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this}}class pE extends La{constructor(t=new ee,n=new ee,a=new ee){super(),this.isQuadraticBezierCurve=!0,this.type="QuadraticBezierCurve",this.v0=t,this.v1=n,this.v2=a}getPoint(t,n=new ee){const a=n,l=this.v0,c=this.v1,f=this.v2;return a.set(yl(t,l.x,c.x,f.x),yl(t,l.y,c.y,f.y)),a}copy(t){return super.copy(t),this.v0.copy(t.v0),this.v1.copy(t.v1),this.v2.copy(t.v2),this}toJSON(){const t=super.toJSON();return t.v0=this.v0.toArray(),t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v0.fromArray(t.v0),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this}}class jx extends La{constructor(t=new k,n=new k,a=new k){super(),this.isQuadraticBezierCurve3=!0,this.type="QuadraticBezierCurve3",this.v0=t,this.v1=n,this.v2=a}getPoint(t,n=new k){const a=n,l=this.v0,c=this.v1,f=this.v2;return a.set(yl(t,l.x,c.x,f.x),yl(t,l.y,c.y,f.y),yl(t,l.z,c.z,f.z)),a}copy(t){return super.copy(t),this.v0.copy(t.v0),this.v1.copy(t.v1),this.v2.copy(t.v2),this}toJSON(){const t=super.toJSON();return t.v0=this.v0.toArray(),t.v1=this.v1.toArray(),t.v2=this.v2.toArray(),t}fromJSON(t){return super.fromJSON(t),this.v0.fromArray(t.v0),this.v1.fromArray(t.v1),this.v2.fromArray(t.v2),this}}class mE extends La{constructor(t=[]){super(),this.isSplineCurve=!0,this.type="SplineCurve",this.points=t}getPoint(t,n=new ee){const a=n,l=this.points,c=(l.length-1)*t,f=Math.floor(c),d=c-f,m=l[f===0?f:f-1],h=l[f],g=l[f>l.length-2?l.length-1:f+1],_=l[f>l.length-3?l.length-1:f+2];return a.set(Mv(d,m.x,h.x,g.x,_.x),Mv(d,m.y,h.y,g.y,_.y)),a}copy(t){super.copy(t),this.points=[];for(let n=0,a=t.points.length;n<a;n++){const l=t.points[n];this.points.push(l.clone())}return this}toJSON(){const t=super.toJSON();t.points=[];for(let n=0,a=this.points.length;n<a;n++){const l=this.points[n];t.points.push(l.toArray())}return t}fromJSON(t){super.fromJSON(t),this.points=[];for(let n=0,a=t.points.length;n<a;n++){const l=t.points[n];this.points.push(new ee().fromArray(l))}return this}}var gE=Object.freeze({__proto__:null,ArcCurve:nE,CatmullRomCurve3:kx,CubicBezierCurve:uE,CubicBezierCurve3:fE,EllipseCurve:Vx,LineCurve:dE,LineCurve3:hE,QuadraticBezierCurve:pE,QuadraticBezierCurve3:jx,SplineCurve:mE});class Pu extends qn{constructor(t=1,n=1,a=1,l=1){super(),this.type="PlaneGeometry",this.parameters={width:t,height:n,widthSegments:a,heightSegments:l};const c=t/2,f=n/2,d=Math.floor(a),m=Math.floor(l),h=d+1,g=m+1,_=t/d,v=n/m,y=[],E=[],A=[],S=[];for(let x=0;x<g;x++){const w=x*v-f;for(let D=0;D<h;D++){const U=D*_-c;E.push(U,-w,0),A.push(0,0,1),S.push(D/d),S.push(1-x/m)}}for(let x=0;x<m;x++)for(let w=0;w<d;w++){const D=w+h*x,U=w+h*(x+1),G=w+1+h*(x+1),O=w+1+h*x;y.push(D,U,O),y.push(U,G,O)}this.setIndex(y),this.setAttribute("position",new Rn(E,3)),this.setAttribute("normal",new Rn(A,3)),this.setAttribute("uv",new Rn(S,2))}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new Pu(t.width,t.height,t.widthSegments,t.heightSegments)}}class eo extends qn{constructor(t=1,n=32,a=16,l=0,c=Math.PI*2,f=0,d=Math.PI){super(),this.type="SphereGeometry",this.parameters={radius:t,widthSegments:n,heightSegments:a,phiStart:l,phiLength:c,thetaStart:f,thetaLength:d},n=Math.max(3,Math.floor(n)),a=Math.max(2,Math.floor(a));const m=Math.min(f+d,Math.PI);let h=0;const g=[],_=new k,v=new k,y=[],E=[],A=[],S=[];for(let x=0;x<=a;x++){const w=[],D=x/a;let U=0;x===0&&f===0?U=.5/n:x===a&&m===Math.PI&&(U=-.5/n);for(let G=0;G<=n;G++){const O=G/n;_.x=-t*Math.cos(l+O*c)*Math.sin(f+D*d),_.y=t*Math.cos(f+D*d),_.z=t*Math.sin(l+O*c)*Math.sin(f+D*d),E.push(_.x,_.y,_.z),v.copy(_).normalize(),A.push(v.x,v.y,v.z),S.push(O+U,1-D),w.push(h++)}g.push(w)}for(let x=0;x<a;x++)for(let w=0;w<n;w++){const D=g[x][w+1],U=g[x][w],G=g[x+1][w],O=g[x+1][w+1];(x!==0||f>0)&&y.push(D,U,O),(x!==a-1||m<Math.PI)&&y.push(U,G,O)}this.setIndex(y),this.setAttribute("position",new Rn(E,3)),this.setAttribute("normal",new Rn(A,3)),this.setAttribute("uv",new Rn(S,2))}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new eo(t.radius,t.widthSegments,t.heightSegments,t.phiStart,t.phiLength,t.thetaStart,t.thetaLength)}}class Cu extends qn{constructor(t=1,n=.4,a=12,l=48,c=Math.PI*2,f=0,d=Math.PI*2){super(),this.type="TorusGeometry",this.parameters={radius:t,tube:n,radialSegments:a,tubularSegments:l,arc:c,thetaStart:f,thetaLength:d},a=Math.floor(a),l=Math.floor(l);const m=[],h=[],g=[],_=[],v=new k,y=new k,E=new k;for(let A=0;A<=a;A++){const S=f+A/a*d;for(let x=0;x<=l;x++){const w=x/l*c;y.x=(t+n*Math.cos(S))*Math.cos(w),y.y=(t+n*Math.cos(S))*Math.sin(w),y.z=n*Math.sin(S),h.push(y.x,y.y,y.z),v.x=t*Math.cos(w),v.y=t*Math.sin(w),E.subVectors(y,v).normalize(),g.push(E.x,E.y,E.z),_.push(x/l),_.push(A/a)}}for(let A=1;A<=a;A++)for(let S=1;S<=l;S++){const x=(l+1)*A+S-1,w=(l+1)*(A-1)+S-1,D=(l+1)*(A-1)+S,U=(l+1)*A+S;m.push(x,w,U),m.push(w,D,U)}this.setIndex(m),this.setAttribute("position",new Rn(h,3)),this.setAttribute("normal",new Rn(g,3)),this.setAttribute("uv",new Rn(_,2))}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new Cu(t.radius,t.tube,t.radialSegments,t.tubularSegments,t.arc)}}class Qp extends qn{constructor(t=new jx(new k(-1,-1,0),new k(-1,1,0),new k(1,1,0)),n=64,a=1,l=8,c=!1){super(),this.type="TubeGeometry",this.parameters={path:t,tubularSegments:n,radius:a,radialSegments:l,closed:c};const f=t.computeFrenetFrames(n,c);this.tangents=f.tangents,this.normals=f.normals,this.binormals=f.binormals;const d=new k,m=new k,h=new ee;let g=new k;const _=[],v=[],y=[],E=[];A(),this.setIndex(E),this.setAttribute("position",new Rn(_,3)),this.setAttribute("normal",new Rn(v,3)),this.setAttribute("uv",new Rn(y,2));function A(){for(let D=0;D<n;D++)S(D);S(c===!1?n:0),w(),x()}function S(D){g=t.getPointAt(D/n,g);const U=f.normals[D],G=f.binormals[D];for(let O=0;O<=l;O++){const B=O/l*Math.PI*2,R=Math.sin(B),z=-Math.cos(B);m.x=z*U.x+R*G.x,m.y=z*U.y+R*G.y,m.z=z*U.z+R*G.z,m.normalize(),v.push(m.x,m.y,m.z),d.x=g.x+a*m.x,d.y=g.y+a*m.y,d.z=g.z+a*m.z,_.push(d.x,d.y,d.z)}}function x(){for(let D=1;D<=n;D++)for(let U=1;U<=l;U++){const G=(l+1)*(D-1)+(U-1),O=(l+1)*D+(U-1),B=(l+1)*D+U,R=(l+1)*(D-1)+U;E.push(G,O,R),E.push(O,B,R)}}function w(){for(let D=0;D<=n;D++)for(let U=0;U<=l;U++)h.x=D/n,h.y=U/l,y.push(h.x,h.y)}}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}toJSON(){const t=super.toJSON();return t.path=this.parameters.path.toJSON(),t}static fromJSON(t){return new Qp(new gE[t.path.type]().fromJSON(t.path),t.tubularSegments,t.radius,t.radialSegments,t.closed)}}function lo(r){const t={};for(const n in r){t[n]={};for(const a in r[n]){const l=r[n][a];if(bv(l))l.isRenderTargetTexture?(ce("UniformsUtils: Textures of render targets cannot be cloned via cloneUniforms() or mergeUniforms()."),t[n][a]=null):t[n][a]=l.clone();else if(Array.isArray(l))if(bv(l[0])){const c=[];for(let f=0,d=l.length;f<d;f++)c[f]=l[f].clone();t[n][a]=c}else t[n][a]=l.slice();else t[n][a]=l}}return t}function Wn(r){const t={};for(let n=0;n<r.length;n++){const a=lo(r[n]);for(const l in a)t[l]=a[l]}return t}function bv(r){return r&&(r.isColor||r.isMatrix3||r.isMatrix4||r.isVector2||r.isVector3||r.isVector4||r.isTexture||r.isQuaternion)}function _E(r){const t=[];for(let n=0;n<r.length;n++)t.push(r[n].clone());return t}function Xx(r){const t=r.getRenderTarget();return t===null?r.outputColorSpace:t.isXRRenderTarget===!0?t.texture.colorSpace:De.workingColorSpace}const vE={clone:lo,merge:Wn};var xE=`void main() {
	gl_Position = projectionMatrix * modelViewMatrix * vec4( position, 1.0 );
}`,yE=`void main() {
	gl_FragColor = vec4( 1.0, 0.0, 0.0, 1.0 );
}`;class na extends er{constructor(t){super(),this.isShaderMaterial=!0,this.type="ShaderMaterial",this.defines={},this.uniforms={},this.uniformsGroups=[],this.vertexShader=xE,this.fragmentShader=yE,this.linewidth=1,this.wireframe=!1,this.wireframeLinewidth=1,this.fog=!1,this.lights=!1,this.clipping=!1,this.forceSinglePass=!0,this.extensions={clipCullDistance:!1,multiDraw:!1},this.defaultAttributeValues={color:[1,1,1],uv:[0,0],uv1:[0,0]},this.index0AttributeName=void 0,this.uniformsNeedUpdate=!1,this.glslVersion=null,t!==void 0&&this.setValues(t)}copy(t){return super.copy(t),this.fragmentShader=t.fragmentShader,this.vertexShader=t.vertexShader,this.uniforms=lo(t.uniforms),this.uniformsGroups=_E(t.uniformsGroups),this.defines=Object.assign({},t.defines),this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.fog=t.fog,this.lights=t.lights,this.clipping=t.clipping,this.extensions=Object.assign({},t.extensions),this.glslVersion=t.glslVersion,this.defaultAttributeValues=Object.assign({},t.defaultAttributeValues),this.index0AttributeName=t.index0AttributeName,this.uniformsNeedUpdate=t.uniformsNeedUpdate,this}toJSON(t){const n=super.toJSON(t);n.glslVersion=this.glslVersion,n.uniforms={};for(const l in this.uniforms){const f=this.uniforms[l].value;f&&f.isTexture?n.uniforms[l]={type:"t",value:f.toJSON(t).uuid}:f&&f.isColor?n.uniforms[l]={type:"c",value:f.getHex()}:f&&f.isVector2?n.uniforms[l]={type:"v2",value:f.toArray()}:f&&f.isVector3?n.uniforms[l]={type:"v3",value:f.toArray()}:f&&f.isVector4?n.uniforms[l]={type:"v4",value:f.toArray()}:f&&f.isMatrix3?n.uniforms[l]={type:"m3",value:f.toArray()}:f&&f.isMatrix4?n.uniforms[l]={type:"m4",value:f.toArray()}:n.uniforms[l]={value:f}}Object.keys(this.defines).length>0&&(n.defines=this.defines),n.vertexShader=this.vertexShader,n.fragmentShader=this.fragmentShader,n.lights=this.lights,n.clipping=this.clipping;const a={};for(const l in this.extensions)this.extensions[l]===!0&&(a[l]=!0);return Object.keys(a).length>0&&(n.extensions=a),n}}class SE extends na{constructor(t){super(t),this.isRawShaderMaterial=!0,this.type="RawShaderMaterial"}}class Ev extends er{constructor(t){super(),this.isMeshStandardMaterial=!0,this.type="MeshStandardMaterial",this.defines={STANDARD:""},this.color=new _e(16777215),this.roughness=1,this.metalness=0,this.map=null,this.lightMap=null,this.lightMapIntensity=1,this.aoMap=null,this.aoMapIntensity=1,this.emissive=new _e(0),this.emissiveIntensity=1,this.emissiveMap=null,this.bumpMap=null,this.bumpScale=1,this.normalMap=null,this.normalMapType=bp,this.normalScale=new ee(1,1),this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.roughnessMap=null,this.metalnessMap=null,this.alphaMap=null,this.envMap=null,this.envMapRotation=new xs,this.envMapIntensity=1,this.wireframe=!1,this.wireframeLinewidth=1,this.wireframeLinecap="round",this.wireframeLinejoin="round",this.flatShading=!1,this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.defines={STANDARD:""},this.color.copy(t.color),this.roughness=t.roughness,this.metalness=t.metalness,this.map=t.map,this.lightMap=t.lightMap,this.lightMapIntensity=t.lightMapIntensity,this.aoMap=t.aoMap,this.aoMapIntensity=t.aoMapIntensity,this.emissive.copy(t.emissive),this.emissiveMap=t.emissiveMap,this.emissiveIntensity=t.emissiveIntensity,this.bumpMap=t.bumpMap,this.bumpScale=t.bumpScale,this.normalMap=t.normalMap,this.normalMapType=t.normalMapType,this.normalScale.copy(t.normalScale),this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this.roughnessMap=t.roughnessMap,this.metalnessMap=t.metalnessMap,this.alphaMap=t.alphaMap,this.envMap=t.envMap,this.envMapRotation.copy(t.envMapRotation),this.envMapIntensity=t.envMapIntensity,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.wireframeLinecap=t.wireframeLinecap,this.wireframeLinejoin=t.wireframeLinejoin,this.flatShading=t.flatShading,this.fog=t.fog,this}}class ME extends er{constructor(t){super(),this.isMeshDepthMaterial=!0,this.type="MeshDepthMaterial",this.depthPacking=nb,this.map=null,this.alphaMap=null,this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.wireframe=!1,this.wireframeLinewidth=1,this.setValues(t)}copy(t){return super.copy(t),this.depthPacking=t.depthPacking,this.map=t.map,this.alphaMap=t.alphaMap,this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this}}class bE extends er{constructor(t){super(),this.isMeshDistanceMaterial=!0,this.type="MeshDistanceMaterial",this.map=null,this.alphaMap=null,this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.setValues(t)}copy(t){return super.copy(t),this.map=t.map,this.alphaMap=t.alphaMap,this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this}}class Wx extends kn{constructor(t,n=1){super(),this.isLight=!0,this.type="Light",this.color=new _e(t),this.intensity=n}dispose(){this.dispatchEvent({type:"dispose"})}copy(t,n){return super.copy(t,n),this.color.copy(t.color),this.intensity=t.intensity,this}toJSON(t){const n=super.toJSON(t);return n.object.color=this.color.getHex(),n.object.intensity=this.intensity,n}}const Ah=new tn,Tv=new k,Av=new k;class EE{constructor(t){this.camera=t,this.intensity=1,this.bias=0,this.biasNode=null,this.normalBias=0,this.radius=1,this.blurSamples=8,this.mapSize=new ee(512,512),this.mapType=pi,this.map=null,this.mapPass=null,this.matrix=new tn,this.autoUpdate=!0,this.needsUpdate=!1,this._frustum=new Zp,this._frameExtents=new ee(1,1),this._viewportCount=1,this._viewports=[new fn(0,0,1,1)]}getViewportCount(){return this._viewportCount}getFrustum(){return this._frustum}updateMatrices(t){const n=this.camera,a=this.matrix;Tv.setFromMatrixPosition(t.matrixWorld),n.position.copy(Tv),Av.setFromMatrixPosition(t.target.matrixWorld),n.lookAt(Av),n.updateMatrixWorld(),Ah.multiplyMatrices(n.projectionMatrix,n.matrixWorldInverse),this._frustum.setFromProjectionMatrix(Ah,n.coordinateSystem,n.reversedDepth),n.coordinateSystem===El||n.reversedDepth?a.set(.5,0,0,.5,0,.5,0,.5,0,0,1,0,0,0,0,1):a.set(.5,0,0,.5,0,.5,0,.5,0,0,.5,.5,0,0,0,1),a.multiply(Ah)}getViewport(t){return this._viewports[t]}getFrameExtents(){return this._frameExtents}dispose(){this.map&&this.map.dispose(),this.mapPass&&this.mapPass.dispose()}copy(t){return this.camera=t.camera.clone(),this.intensity=t.intensity,this.bias=t.bias,this.radius=t.radius,this.autoUpdate=t.autoUpdate,this.needsUpdate=t.needsUpdate,this.normalBias=t.normalBias,this.blurSamples=t.blurSamples,this.mapSize.copy(t.mapSize),this.biasNode=t.biasNode,this}clone(){return new this.constructor().copy(this)}toJSON(){const t={};return this.intensity!==1&&(t.intensity=this.intensity),this.bias!==0&&(t.bias=this.bias),this.normalBias!==0&&(t.normalBias=this.normalBias),this.radius!==1&&(t.radius=this.radius),(this.mapSize.x!==512||this.mapSize.y!==512)&&(t.mapSize=this.mapSize.toArray()),t.camera=this.camera.toJSON(!1).object,delete t.camera.matrix,t}}const lu=new k,cu=new wi,Wi=new k;class qx extends kn{constructor(){super(),this.isCamera=!0,this.type="Camera",this.matrixWorldInverse=new tn,this.projectionMatrix=new tn,this.projectionMatrixInverse=new tn,this.coordinateSystem=Ji,this._reversedDepth=!1}get reversedDepth(){return this._reversedDepth}copy(t,n){return super.copy(t,n),this.matrixWorldInverse.copy(t.matrixWorldInverse),this.projectionMatrix.copy(t.projectionMatrix),this.projectionMatrixInverse.copy(t.projectionMatrixInverse),this.coordinateSystem=t.coordinateSystem,this}getWorldDirection(t){return super.getWorldDirection(t).negate()}updateMatrixWorld(t){super.updateMatrixWorld(t),this.matrixWorld.decompose(lu,cu,Wi),Wi.x===1&&Wi.y===1&&Wi.z===1?this.matrixWorldInverse.copy(this.matrixWorld).invert():this.matrixWorldInverse.compose(lu,cu,Wi.set(1,1,1)).invert()}updateWorldMatrix(t,n){super.updateWorldMatrix(t,n),this.matrixWorld.decompose(lu,cu,Wi),Wi.x===1&&Wi.y===1&&Wi.z===1?this.matrixWorldInverse.copy(this.matrixWorld).invert():this.matrixWorldInverse.compose(lu,cu,Wi.set(1,1,1)).invert()}clone(){return new this.constructor().copy(this)}}const ps=new k,wv=new ee,Rv=new ee;class hi extends qx{constructor(t=50,n=1,a=.1,l=2e3){super(),this.isPerspectiveCamera=!0,this.type="PerspectiveCamera",this.fov=t,this.zoom=1,this.near=a,this.far=l,this.focus=10,this.aspect=n,this.view=null,this.filmGauge=35,this.filmOffset=0,this.updateProjectionMatrix()}copy(t,n){return super.copy(t,n),this.fov=t.fov,this.zoom=t.zoom,this.near=t.near,this.far=t.far,this.focus=t.focus,this.aspect=t.aspect,this.view=t.view===null?null:Object.assign({},t.view),this.filmGauge=t.filmGauge,this.filmOffset=t.filmOffset,this}setFocalLength(t){const n=.5*this.getFilmHeight()/t;this.fov=Tl*2*Math.atan(n),this.updateProjectionMatrix()}getFocalLength(){const t=Math.tan(vl*.5*this.fov);return .5*this.getFilmHeight()/t}getEffectiveFOV(){return Tl*2*Math.atan(Math.tan(vl*.5*this.fov)/this.zoom)}getFilmWidth(){return this.filmGauge*Math.min(this.aspect,1)}getFilmHeight(){return this.filmGauge/Math.max(this.aspect,1)}getViewBounds(t,n,a){ps.set(-1,-1,.5).applyMatrix4(this.projectionMatrixInverse),n.set(ps.x,ps.y).multiplyScalar(-t/ps.z),ps.set(1,1,.5).applyMatrix4(this.projectionMatrixInverse),a.set(ps.x,ps.y).multiplyScalar(-t/ps.z)}getViewSize(t,n){return this.getViewBounds(t,wv,Rv),n.subVectors(Rv,wv)}setViewOffset(t,n,a,l,c,f){this.aspect=t/n,this.view===null&&(this.view={enabled:!0,fullWidth:1,fullHeight:1,offsetX:0,offsetY:0,width:1,height:1}),this.view.enabled=!0,this.view.fullWidth=t,this.view.fullHeight=n,this.view.offsetX=a,this.view.offsetY=l,this.view.width=c,this.view.height=f,this.updateProjectionMatrix()}clearViewOffset(){this.view!==null&&(this.view.enabled=!1),this.updateProjectionMatrix()}updateProjectionMatrix(){const t=this.near;let n=t*Math.tan(vl*.5*this.fov)/this.zoom,a=2*n,l=this.aspect*a,c=-.5*l;const f=this.view;if(this.view!==null&&this.view.enabled){const m=f.fullWidth,h=f.fullHeight;c+=f.offsetX*l/m,n-=f.offsetY*a/h,l*=f.width/m,a*=f.height/h}const d=this.filmOffset;d!==0&&(c+=t*d/this.getFilmWidth()),this.projectionMatrix.makePerspective(c,c+l,n,n-a,t,this.far,this.coordinateSystem,this.reversedDepth),this.projectionMatrixInverse.copy(this.projectionMatrix).invert()}toJSON(t){const n=super.toJSON(t);return n.object.fov=this.fov,n.object.zoom=this.zoom,n.object.near=this.near,n.object.far=this.far,n.object.focus=this.focus,n.object.aspect=this.aspect,this.view!==null&&(n.object.view=Object.assign({},this.view)),n.object.filmGauge=this.filmGauge,n.object.filmOffset=this.filmOffset,n}}class TE extends EE{constructor(){super(new hi(90,1,.5,500)),this.isPointLightShadow=!0}}class AE extends Wx{constructor(t,n,a=0,l=2){super(t,n),this.isPointLight=!0,this.type="PointLight",this.distance=a,this.decay=l,this.shadow=new TE}get power(){return this.intensity*4*Math.PI}set power(t){this.intensity=t/(4*Math.PI)}dispose(){super.dispose(),this.shadow.dispose()}copy(t,n){return super.copy(t,n),this.distance=t.distance,this.decay=t.decay,this.shadow=t.shadow.clone(),this}toJSON(t){const n=super.toJSON(t);return n.object.distance=this.distance,n.object.decay=this.decay,n.object.shadow=this.shadow.toJSON(),n}}class Yx extends qx{constructor(t=-1,n=1,a=1,l=-1,c=.1,f=2e3){super(),this.isOrthographicCamera=!0,this.type="OrthographicCamera",this.zoom=1,this.view=null,this.left=t,this.right=n,this.top=a,this.bottom=l,this.near=c,this.far=f,this.updateProjectionMatrix()}copy(t,n){return super.copy(t,n),this.left=t.left,this.right=t.right,this.top=t.top,this.bottom=t.bottom,this.near=t.near,this.far=t.far,this.zoom=t.zoom,this.view=t.view===null?null:Object.assign({},t.view),this}setViewOffset(t,n,a,l,c,f){this.view===null&&(this.view={enabled:!0,fullWidth:1,fullHeight:1,offsetX:0,offsetY:0,width:1,height:1}),this.view.enabled=!0,this.view.fullWidth=t,this.view.fullHeight=n,this.view.offsetX=a,this.view.offsetY=l,this.view.width=c,this.view.height=f,this.updateProjectionMatrix()}clearViewOffset(){this.view!==null&&(this.view.enabled=!1),this.updateProjectionMatrix()}updateProjectionMatrix(){const t=(this.right-this.left)/(2*this.zoom),n=(this.top-this.bottom)/(2*this.zoom),a=(this.right+this.left)/2,l=(this.top+this.bottom)/2;let c=a-t,f=a+t,d=l+n,m=l-n;if(this.view!==null&&this.view.enabled){const h=(this.right-this.left)/this.view.fullWidth/this.zoom,g=(this.top-this.bottom)/this.view.fullHeight/this.zoom;c+=h*this.view.offsetX,f=c+h*this.view.width,d-=g*this.view.offsetY,m=d-g*this.view.height}this.projectionMatrix.makeOrthographic(c,f,d,m,this.near,this.far,this.coordinateSystem,this.reversedDepth),this.projectionMatrixInverse.copy(this.projectionMatrix).invert()}toJSON(t){const n=super.toJSON(t);return n.object.zoom=this.zoom,n.object.left=this.left,n.object.right=this.right,n.object.top=this.top,n.object.bottom=this.bottom,n.object.near=this.near,n.object.far=this.far,this.view!==null&&(n.object.view=Object.assign({},this.view)),n}}class wE extends Wx{constructor(t,n){super(t,n),this.isAmbientLight=!0,this.type="AmbientLight"}}const Kr=-90,Qr=1;class RE extends kn{constructor(t,n,a){super(),this.type="CubeCamera",this.renderTarget=a,this.coordinateSystem=null,this.activeMipmapLevel=0;const l=new hi(Kr,Qr,t,n);l.layers=this.layers,this.add(l);const c=new hi(Kr,Qr,t,n);c.layers=this.layers,this.add(c);const f=new hi(Kr,Qr,t,n);f.layers=this.layers,this.add(f);const d=new hi(Kr,Qr,t,n);d.layers=this.layers,this.add(d);const m=new hi(Kr,Qr,t,n);m.layers=this.layers,this.add(m);const h=new hi(Kr,Qr,t,n);h.layers=this.layers,this.add(h)}updateCoordinateSystem(){const t=this.coordinateSystem,n=this.children.concat(),[a,l,c,f,d,m]=n;for(const h of n)this.remove(h);if(t===Ji)a.up.set(0,1,0),a.lookAt(1,0,0),l.up.set(0,1,0),l.lookAt(-1,0,0),c.up.set(0,0,-1),c.lookAt(0,1,0),f.up.set(0,0,1),f.lookAt(0,-1,0),d.up.set(0,1,0),d.lookAt(0,0,1),m.up.set(0,1,0),m.lookAt(0,0,-1);else if(t===El)a.up.set(0,-1,0),a.lookAt(-1,0,0),l.up.set(0,-1,0),l.lookAt(1,0,0),c.up.set(0,0,1),c.lookAt(0,1,0),f.up.set(0,0,-1),f.lookAt(0,-1,0),d.up.set(0,-1,0),d.lookAt(0,0,1),m.up.set(0,-1,0),m.lookAt(0,0,-1);else throw new Error("THREE.CubeCamera.updateCoordinateSystem(): Invalid coordinate system: "+t);for(const h of n)this.add(h),h.updateMatrixWorld()}update(t,n){this.parent===null&&this.updateMatrixWorld();const{renderTarget:a,activeMipmapLevel:l}=this;this.coordinateSystem!==t.coordinateSystem&&(this.coordinateSystem=t.coordinateSystem,this.updateCoordinateSystem());const[c,f,d,m,h,g]=this.children,_=t.getRenderTarget(),v=t.getActiveCubeFace(),y=t.getActiveMipmapLevel(),E=t.xr.enabled;t.xr.enabled=!1;const A=a.texture.generateMipmaps;a.texture.generateMipmaps=!1;let S=!1;t.isWebGLRenderer===!0?S=t.state.buffers.depth.getReversed():S=t.reversedDepthBuffer,t.setRenderTarget(a,0,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,c),t.setRenderTarget(a,1,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,f),t.setRenderTarget(a,2,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,d),t.setRenderTarget(a,3,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,m),t.setRenderTarget(a,4,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,h),a.texture.generateMipmaps=A,t.setRenderTarget(a,5,l),S&&t.autoClear===!1&&t.clearDepth(),t.render(n,g),t.setRenderTarget(_,v,y),t.xr.enabled=E,a.texture.needsPMREMUpdate=!0}}class CE extends hi{constructor(t=[]){super(),this.isArrayCamera=!0,this.isMultiViewCamera=!1,this.cameras=t}}const Cv=new tn;class NE{constructor(t,n,a=0,l=1/0){this.ray=new Yp(t,n),this.near=a,this.far=l,this.camera=null,this.layers=new qp,this.params={Mesh:{},Line:{threshold:1},LOD:{},Points:{threshold:1},Sprite:{}}}set(t,n){this.ray.set(t,n)}setFromCamera(t,n){n.isPerspectiveCamera?(this.ray.origin.setFromMatrixPosition(n.matrixWorld),this.ray.direction.set(t.x,t.y,.5).unproject(n).sub(this.ray.origin).normalize(),this.camera=n):n.isOrthographicCamera?(this.ray.origin.set(t.x,t.y,(n.near+n.far)/(n.near-n.far)).unproject(n),this.ray.direction.set(0,0,-1).transformDirection(n.matrixWorld),this.camera=n):Ne("Raycaster: Unsupported camera type: "+n.type)}setFromXRController(t){return Cv.identity().extractRotation(t.matrixWorld),this.ray.origin.setFromMatrixPosition(t.matrixWorld),this.ray.direction.set(0,0,-1).applyMatrix4(Cv),this}intersectObject(t,n=!0,a=[]){return wp(t,this,a,n),a.sort(Nv),a}intersectObjects(t,n=!0,a=[]){for(let l=0,c=t.length;l<c;l++)wp(t[l],this,a,n);return a.sort(Nv),a}}function Nv(r,t){return r.distance-t.distance}function wp(r,t,n,a){let l=!0;if(r.layers.test(t.layers)&&r.raycast(t,n)===!1&&(l=!1),l===!0&&a===!0){const c=r.children;for(let f=0,d=c.length;f<d;f++)wp(c[f],t,n,!0)}}class Zx{static{Zx.prototype.isMatrix2=!0}constructor(t,n,a,l){this.elements=[1,0,0,1],t!==void 0&&this.set(t,n,a,l)}identity(){return this.set(1,0,0,1),this}fromArray(t,n=0){for(let a=0;a<4;a++)this.elements[a]=t[a+n];return this}set(t,n,a,l){const c=this.elements;return c[0]=t,c[2]=n,c[1]=a,c[3]=l,this}}function Dv(r,t,n,a){const l=DE(a);switch(n){case Nx:return r*t;case Ux:return r*t/l.components*l.byteLength;case Hp:return r*t/l.components*l.byteLength;case $s:return r*t*2/l.components*l.byteLength;case Gp:return r*t*2/l.components*l.byteLength;case Dx:return r*t*3/l.components*l.byteLength;case Hi:return r*t*4/l.components*l.byteLength;case Vp:return r*t*4/l.components*l.byteLength;case mu:case gu:return Math.floor((r+3)/4)*Math.floor((t+3)/4)*8;case _u:case vu:return Math.floor((r+3)/4)*Math.floor((t+3)/4)*16;case Yh:case Kh:return Math.max(r,16)*Math.max(t,8)/4;case qh:case Zh:return Math.max(r,8)*Math.max(t,8)/2;case Qh:case Jh:case tp:case ep:return Math.floor((r+3)/4)*Math.floor((t+3)/4)*8;case $h:case Mu:case np:return Math.floor((r+3)/4)*Math.floor((t+3)/4)*16;case ip:return Math.floor((r+3)/4)*Math.floor((t+3)/4)*16;case ap:return Math.floor((r+4)/5)*Math.floor((t+3)/4)*16;case sp:return Math.floor((r+4)/5)*Math.floor((t+4)/5)*16;case rp:return Math.floor((r+5)/6)*Math.floor((t+4)/5)*16;case op:return Math.floor((r+5)/6)*Math.floor((t+5)/6)*16;case lp:return Math.floor((r+7)/8)*Math.floor((t+4)/5)*16;case cp:return Math.floor((r+7)/8)*Math.floor((t+5)/6)*16;case up:return Math.floor((r+7)/8)*Math.floor((t+7)/8)*16;case fp:return Math.floor((r+9)/10)*Math.floor((t+4)/5)*16;case dp:return Math.floor((r+9)/10)*Math.floor((t+5)/6)*16;case hp:return Math.floor((r+9)/10)*Math.floor((t+7)/8)*16;case pp:return Math.floor((r+9)/10)*Math.floor((t+9)/10)*16;case mp:return Math.floor((r+11)/12)*Math.floor((t+9)/10)*16;case gp:return Math.floor((r+11)/12)*Math.floor((t+11)/12)*16;case _p:case vp:case xp:return Math.ceil(r/4)*Math.ceil(t/4)*16;case yp:case Sp:return Math.ceil(r/4)*Math.ceil(t/4)*8;case bu:case Mp:return Math.ceil(r/4)*Math.ceil(t/4)*16}throw new Error(`Unable to determine texture byte length for ${n} format.`)}function DE(r){switch(r){case pi:case Ax:return{byteLength:1,components:1};case Ml:case wx:case Da:return{byteLength:2,components:1};case Bp:case Fp:return{byteLength:2,components:4};case ea:case zp:case Qi:return{byteLength:4,components:1};case Rx:case Cx:return{byteLength:4,components:3}}throw new Error(`Unknown texture type ${r}.`)}typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("register",{detail:{revision:Ip}}));typeof window<"u"&&(window.__THREE__?ce("WARNING: Multiple instances of Three.js being imported."):window.__THREE__=Ip);/**
 * @license
 * Copyright 2010-2026 Three.js Authors
 * SPDX-License-Identifier: MIT
 */function Kx(){let r=null,t=!1,n=null,a=null;function l(c,f){n(c,f),a=r.requestAnimationFrame(l)}return{start:function(){t!==!0&&n!==null&&r!==null&&(a=r.requestAnimationFrame(l),t=!0)},stop:function(){r!==null&&r.cancelAnimationFrame(a),t=!1},setAnimationLoop:function(c){n=c},setContext:function(c){r=c}}}function UE(r){const t=new WeakMap;function n(d,m){const h=d.array,g=d.usage,_=h.byteLength,v=r.createBuffer();r.bindBuffer(m,v),r.bufferData(m,h,g),d.onUploadCallback();let y;if(h instanceof Float32Array)y=r.FLOAT;else if(typeof Float16Array<"u"&&h instanceof Float16Array)y=r.HALF_FLOAT;else if(h instanceof Uint16Array)d.isFloat16BufferAttribute?y=r.HALF_FLOAT:y=r.UNSIGNED_SHORT;else if(h instanceof Int16Array)y=r.SHORT;else if(h instanceof Uint32Array)y=r.UNSIGNED_INT;else if(h instanceof Int32Array)y=r.INT;else if(h instanceof Int8Array)y=r.BYTE;else if(h instanceof Uint8Array)y=r.UNSIGNED_BYTE;else if(h instanceof Uint8ClampedArray)y=r.UNSIGNED_BYTE;else throw new Error("THREE.WebGLAttributes: Unsupported buffer data format: "+h);return{buffer:v,type:y,bytesPerElement:h.BYTES_PER_ELEMENT,version:d.version,size:_}}function a(d,m,h){const g=m.array,_=m.updateRanges;if(r.bindBuffer(h,d),_.length===0)r.bufferSubData(h,0,g);else{_.sort((y,E)=>y.start-E.start);let v=0;for(let y=1;y<_.length;y++){const E=_[v],A=_[y];A.start<=E.start+E.count+1?E.count=Math.max(E.count,A.start+A.count-E.start):(++v,_[v]=A)}_.length=v+1;for(let y=0,E=_.length;y<E;y++){const A=_[y];r.bufferSubData(h,A.start*g.BYTES_PER_ELEMENT,g,A.start,A.count)}m.clearUpdateRanges()}m.onUploadCallback()}function l(d){return d.isInterleavedBufferAttribute&&(d=d.data),t.get(d)}function c(d){d.isInterleavedBufferAttribute&&(d=d.data);const m=t.get(d);m&&(r.deleteBuffer(m.buffer),t.delete(d))}function f(d,m){if(d.isInterleavedBufferAttribute&&(d=d.data),d.isGLBufferAttribute){const g=t.get(d);(!g||g.version<d.version)&&t.set(d,{buffer:d.buffer,type:d.type,bytesPerElement:d.elementSize,version:d.version});return}const h=t.get(d);if(h===void 0)t.set(d,n(d,m));else if(h.version<d.version){if(h.size!==d.array.byteLength)throw new Error("THREE.WebGLAttributes: The size of the buffer attribute's array buffer does not match the original size. Resizing buffer attributes is not supported.");a(h.buffer,d,m),h.version=d.version}}return{get:l,remove:c,update:f}}var LE=`#ifdef USE_ALPHAHASH
	if ( diffuseColor.a < getAlphaHashThreshold( vPosition ) ) discard;
#endif`,OE=`#ifdef USE_ALPHAHASH
	const float ALPHA_HASH_SCALE = 0.05;
	float hash2D( vec2 value ) {
		return fract( 1.0e4 * sin( 17.0 * value.x + 0.1 * value.y ) * ( 0.1 + abs( sin( 13.0 * value.y + value.x ) ) ) );
	}
	float hash3D( vec3 value ) {
		return hash2D( vec2( hash2D( value.xy ), value.z ) );
	}
	float getAlphaHashThreshold( vec3 position ) {
		float maxDeriv = max(
			length( dFdx( position.xyz ) ),
			length( dFdy( position.xyz ) )
		);
		float pixScale = 1.0 / ( ALPHA_HASH_SCALE * maxDeriv );
		vec2 pixScales = vec2(
			exp2( floor( log2( pixScale ) ) ),
			exp2( ceil( log2( pixScale ) ) )
		);
		vec2 alpha = vec2(
			hash3D( floor( pixScales.x * position.xyz ) ),
			hash3D( floor( pixScales.y * position.xyz ) )
		);
		float lerpFactor = fract( log2( pixScale ) );
		float x = ( 1.0 - lerpFactor ) * alpha.x + lerpFactor * alpha.y;
		float a = min( lerpFactor, 1.0 - lerpFactor );
		vec3 cases = vec3(
			x * x / ( 2.0 * a * ( 1.0 - a ) ),
			( x - 0.5 * a ) / ( 1.0 - a ),
			1.0 - ( ( 1.0 - x ) * ( 1.0 - x ) / ( 2.0 * a * ( 1.0 - a ) ) )
		);
		float threshold = ( x < ( 1.0 - a ) )
			? ( ( x < a ) ? cases.x : cases.y )
			: cases.z;
		return clamp( threshold , 1.0e-6, 1.0 );
	}
#endif`,PE=`#ifdef USE_ALPHAMAP
	diffuseColor.a *= texture2D( alphaMap, vAlphaMapUv ).g;
#endif`,IE=`#ifdef USE_ALPHAMAP
	uniform sampler2D alphaMap;
#endif`,zE=`#ifdef USE_ALPHATEST
	#ifdef ALPHA_TO_COVERAGE
	diffuseColor.a = smoothstep( alphaTest, alphaTest + fwidth( diffuseColor.a ), diffuseColor.a );
	if ( diffuseColor.a == 0.0 ) discard;
	#else
	if ( diffuseColor.a < alphaTest ) discard;
	#endif
#endif`,BE=`#ifdef USE_ALPHATEST
	uniform float alphaTest;
#endif`,FE=`#ifdef USE_AOMAP
	float ambientOcclusion = ( texture2D( aoMap, vAoMapUv ).r - 1.0 ) * aoMapIntensity + 1.0;
	reflectedLight.indirectDiffuse *= ambientOcclusion;
	#if defined( USE_CLEARCOAT ) 
		clearcoatSpecularIndirect *= ambientOcclusion;
	#endif
	#if defined( USE_SHEEN ) 
		sheenSpecularIndirect *= ambientOcclusion;
	#endif
	#if defined( USE_ENVMAP ) && defined( STANDARD )
		float dotNV = saturate( dot( geometryNormal, geometryViewDir ) );
		reflectedLight.indirectSpecular *= computeSpecularOcclusion( dotNV, ambientOcclusion, material.roughness );
	#endif
#endif`,HE=`#ifdef USE_AOMAP
	uniform sampler2D aoMap;
	uniform float aoMapIntensity;
#endif`,GE=`#ifdef USE_BATCHING
	#if ! defined( GL_ANGLE_multi_draw )
	#define gl_DrawID _gl_DrawID
	uniform int _gl_DrawID;
	#endif
	uniform highp sampler2D batchingTexture;
	uniform highp usampler2D batchingIdTexture;
	mat4 getBatchingMatrix( const in float i ) {
		int size = textureSize( batchingTexture, 0 ).x;
		int j = int( i ) * 4;
		int x = j % size;
		int y = j / size;
		vec4 v1 = texelFetch( batchingTexture, ivec2( x, y ), 0 );
		vec4 v2 = texelFetch( batchingTexture, ivec2( x + 1, y ), 0 );
		vec4 v3 = texelFetch( batchingTexture, ivec2( x + 2, y ), 0 );
		vec4 v4 = texelFetch( batchingTexture, ivec2( x + 3, y ), 0 );
		return mat4( v1, v2, v3, v4 );
	}
	float getIndirectIndex( const in int i ) {
		int size = textureSize( batchingIdTexture, 0 ).x;
		int x = i % size;
		int y = i / size;
		return float( texelFetch( batchingIdTexture, ivec2( x, y ), 0 ).r );
	}
#endif
#ifdef USE_BATCHING_COLOR
	uniform sampler2D batchingColorTexture;
	vec4 getBatchingColor( const in float i ) {
		int size = textureSize( batchingColorTexture, 0 ).x;
		int j = int( i );
		int x = j % size;
		int y = j / size;
		return texelFetch( batchingColorTexture, ivec2( x, y ), 0 );
	}
#endif`,VE=`#ifdef USE_BATCHING
	mat4 batchingMatrix = getBatchingMatrix( getIndirectIndex( gl_DrawID ) );
#endif`,kE=`vec3 transformed = vec3( position );
#ifdef USE_ALPHAHASH
	vPosition = vec3( position );
#endif`,jE=`vec3 objectNormal = vec3( normal );
#ifdef USE_TANGENT
	vec3 objectTangent = vec3( tangent.xyz );
#endif`,XE=`float G_BlinnPhong_Implicit( ) {
	return 0.25;
}
float D_BlinnPhong( const in float shininess, const in float dotNH ) {
	return RECIPROCAL_PI * ( shininess * 0.5 + 1.0 ) * pow( dotNH, shininess );
}
vec3 BRDF_BlinnPhong( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in vec3 specularColor, const in float shininess ) {
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNH = saturate( dot( normal, halfDir ) );
	float dotVH = saturate( dot( viewDir, halfDir ) );
	vec3 F = F_Schlick( specularColor, 1.0, dotVH );
	float G = G_BlinnPhong_Implicit( );
	float D = D_BlinnPhong( shininess, dotNH );
	return F * ( G * D );
} // validated`,WE=`#ifdef USE_IRIDESCENCE
	const mat3 XYZ_TO_REC709 = mat3(
		 3.2404542, -0.9692660,  0.0556434,
		-1.5371385,  1.8760108, -0.2040259,
		-0.4985314,  0.0415560,  1.0572252
	);
	vec3 Fresnel0ToIor( vec3 fresnel0 ) {
		vec3 sqrtF0 = sqrt( fresnel0 );
		return ( vec3( 1.0 ) + sqrtF0 ) / ( vec3( 1.0 ) - sqrtF0 );
	}
	vec3 IorToFresnel0( vec3 transmittedIor, float incidentIor ) {
		return pow2( ( transmittedIor - vec3( incidentIor ) ) / ( transmittedIor + vec3( incidentIor ) ) );
	}
	float IorToFresnel0( float transmittedIor, float incidentIor ) {
		return pow2( ( transmittedIor - incidentIor ) / ( transmittedIor + incidentIor ));
	}
	vec3 evalSensitivity( float OPD, vec3 shift ) {
		float phase = 2.0 * PI * OPD * 1.0e-9;
		vec3 val = vec3( 5.4856e-13, 4.4201e-13, 5.2481e-13 );
		vec3 pos = vec3( 1.6810e+06, 1.7953e+06, 2.2084e+06 );
		vec3 var = vec3( 4.3278e+09, 9.3046e+09, 6.6121e+09 );
		vec3 xyz = val * sqrt( 2.0 * PI * var ) * cos( pos * phase + shift ) * exp( - pow2( phase ) * var );
		xyz.x += 9.7470e-14 * sqrt( 2.0 * PI * 4.5282e+09 ) * cos( 2.2399e+06 * phase + shift[ 0 ] ) * exp( - 4.5282e+09 * pow2( phase ) );
		xyz /= 1.0685e-7;
		vec3 rgb = XYZ_TO_REC709 * xyz;
		return rgb;
	}
	vec3 evalIridescence( float outsideIOR, float eta2, float cosTheta1, float thinFilmThickness, vec3 baseF0 ) {
		vec3 I;
		float iridescenceIOR = mix( outsideIOR, eta2, smoothstep( 0.0, 0.03, thinFilmThickness ) );
		float sinTheta2Sq = pow2( outsideIOR / iridescenceIOR ) * ( 1.0 - pow2( cosTheta1 ) );
		float cosTheta2Sq = 1.0 - sinTheta2Sq;
		if ( cosTheta2Sq < 0.0 ) {
			return vec3( 1.0 );
		}
		float cosTheta2 = sqrt( cosTheta2Sq );
		float R0 = IorToFresnel0( iridescenceIOR, outsideIOR );
		float R12 = F_Schlick( R0, 1.0, cosTheta1 );
		float T121 = 1.0 - R12;
		float phi12 = 0.0;
		if ( iridescenceIOR < outsideIOR ) phi12 = PI;
		float phi21 = PI - phi12;
		vec3 baseIOR = Fresnel0ToIor( clamp( baseF0, 0.0, 0.9999 ) );		vec3 R1 = IorToFresnel0( baseIOR, iridescenceIOR );
		vec3 R23 = F_Schlick( R1, 1.0, cosTheta2 );
		vec3 phi23 = vec3( 0.0 );
		if ( baseIOR[ 0 ] < iridescenceIOR ) phi23[ 0 ] = PI;
		if ( baseIOR[ 1 ] < iridescenceIOR ) phi23[ 1 ] = PI;
		if ( baseIOR[ 2 ] < iridescenceIOR ) phi23[ 2 ] = PI;
		float OPD = 2.0 * iridescenceIOR * thinFilmThickness * cosTheta2;
		vec3 phi = vec3( phi21 ) + phi23;
		vec3 R123 = clamp( R12 * R23, 1e-5, 0.9999 );
		vec3 r123 = sqrt( R123 );
		vec3 Rs = pow2( T121 ) * R23 / ( vec3( 1.0 ) - R123 );
		vec3 C0 = R12 + Rs;
		I = C0;
		vec3 Cm = Rs - T121;
		for ( int m = 1; m <= 2; ++ m ) {
			Cm *= r123;
			vec3 Sm = 2.0 * evalSensitivity( float( m ) * OPD, float( m ) * phi );
			I += Cm * Sm;
		}
		return max( I, vec3( 0.0 ) );
	}
#endif`,qE=`#ifdef USE_BUMPMAP
	uniform sampler2D bumpMap;
	uniform float bumpScale;
	vec2 dHdxy_fwd() {
		vec2 dSTdx = dFdx( vBumpMapUv );
		vec2 dSTdy = dFdy( vBumpMapUv );
		float Hll = bumpScale * texture2D( bumpMap, vBumpMapUv ).x;
		float dBx = bumpScale * texture2D( bumpMap, vBumpMapUv + dSTdx ).x - Hll;
		float dBy = bumpScale * texture2D( bumpMap, vBumpMapUv + dSTdy ).x - Hll;
		return vec2( dBx, dBy );
	}
	vec3 perturbNormalArb( vec3 surf_pos, vec3 surf_norm, vec2 dHdxy, float faceDirection ) {
		vec3 vSigmaX = normalize( dFdx( surf_pos.xyz ) );
		vec3 vSigmaY = normalize( dFdy( surf_pos.xyz ) );
		vec3 vN = surf_norm;
		vec3 R1 = cross( vSigmaY, vN );
		vec3 R2 = cross( vN, vSigmaX );
		float fDet = dot( vSigmaX, R1 ) * faceDirection;
		vec3 vGrad = sign( fDet ) * ( dHdxy.x * R1 + dHdxy.y * R2 );
		return normalize( abs( fDet ) * surf_norm - vGrad );
	}
#endif`,YE=`#if NUM_CLIPPING_PLANES > 0
	vec4 plane;
	#ifdef ALPHA_TO_COVERAGE
		float distanceToPlane, distanceGradient;
		float clipOpacity = 1.0;
		#pragma unroll_loop_start
		for ( int i = 0; i < UNION_CLIPPING_PLANES; i ++ ) {
			plane = clippingPlanes[ i ];
			distanceToPlane = - dot( vClipPosition, plane.xyz ) + plane.w;
			distanceGradient = fwidth( distanceToPlane ) / 2.0;
			clipOpacity *= smoothstep( - distanceGradient, distanceGradient, distanceToPlane );
			if ( clipOpacity == 0.0 ) discard;
		}
		#pragma unroll_loop_end
		#if UNION_CLIPPING_PLANES < NUM_CLIPPING_PLANES
			float unionClipOpacity = 1.0;
			#pragma unroll_loop_start
			for ( int i = UNION_CLIPPING_PLANES; i < NUM_CLIPPING_PLANES; i ++ ) {
				plane = clippingPlanes[ i ];
				distanceToPlane = - dot( vClipPosition, plane.xyz ) + plane.w;
				distanceGradient = fwidth( distanceToPlane ) / 2.0;
				unionClipOpacity *= 1.0 - smoothstep( - distanceGradient, distanceGradient, distanceToPlane );
			}
			#pragma unroll_loop_end
			clipOpacity *= 1.0 - unionClipOpacity;
		#endif
		diffuseColor.a *= clipOpacity;
		if ( diffuseColor.a == 0.0 ) discard;
	#else
		#pragma unroll_loop_start
		for ( int i = 0; i < UNION_CLIPPING_PLANES; i ++ ) {
			plane = clippingPlanes[ i ];
			if ( dot( vClipPosition, plane.xyz ) > plane.w ) discard;
		}
		#pragma unroll_loop_end
		#if UNION_CLIPPING_PLANES < NUM_CLIPPING_PLANES
			bool clipped = true;
			#pragma unroll_loop_start
			for ( int i = UNION_CLIPPING_PLANES; i < NUM_CLIPPING_PLANES; i ++ ) {
				plane = clippingPlanes[ i ];
				clipped = ( dot( vClipPosition, plane.xyz ) > plane.w ) && clipped;
			}
			#pragma unroll_loop_end
			if ( clipped ) discard;
		#endif
	#endif
#endif`,ZE=`#if NUM_CLIPPING_PLANES > 0
	varying vec3 vClipPosition;
	uniform vec4 clippingPlanes[ NUM_CLIPPING_PLANES ];
#endif`,KE=`#if NUM_CLIPPING_PLANES > 0
	varying vec3 vClipPosition;
#endif`,QE=`#if NUM_CLIPPING_PLANES > 0
	vClipPosition = - mvPosition.xyz;
#endif`,JE=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA )
	diffuseColor *= vColor;
#endif`,$E=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA )
	varying vec4 vColor;
#endif`,tT=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA ) || defined( USE_INSTANCING_COLOR ) || defined( USE_BATCHING_COLOR )
	varying vec4 vColor;
#endif`,eT=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA ) || defined( USE_INSTANCING_COLOR ) || defined( USE_BATCHING_COLOR )
	vColor = vec4( 1.0 );
#endif
#ifdef USE_COLOR_ALPHA
	vColor *= color;
#elif defined( USE_COLOR )
	vColor.rgb *= color;
#endif
#ifdef USE_INSTANCING_COLOR
	vColor.rgb *= instanceColor.rgb;
#endif
#ifdef USE_BATCHING_COLOR
	vColor *= getBatchingColor( getIndirectIndex( gl_DrawID ) );
#endif`,nT=`#define PI 3.141592653589793
#define PI2 6.283185307179586
#define PI_HALF 1.5707963267948966
#define RECIPROCAL_PI 0.3183098861837907
#define RECIPROCAL_PI2 0.15915494309189535
#define EPSILON 1e-6
#ifndef saturate
#define saturate( a ) clamp( a, 0.0, 1.0 )
#endif
#define whiteComplement( a ) ( 1.0 - saturate( a ) )
float pow2( const in float x ) { return x*x; }
vec3 pow2( const in vec3 x ) { return x*x; }
float pow3( const in float x ) { return x*x*x; }
float pow4( const in float x ) { float x2 = x*x; return x2*x2; }
float max3( const in vec3 v ) { return max( max( v.x, v.y ), v.z ); }
float average( const in vec3 v ) { return dot( v, vec3( 0.3333333 ) ); }
highp float rand( const in vec2 uv ) {
	const highp float a = 12.9898, b = 78.233, c = 43758.5453;
	highp float dt = dot( uv.xy, vec2( a,b ) ), sn = mod( dt, PI );
	return fract( sin( sn ) * c );
}
#ifdef HIGH_PRECISION
	float precisionSafeLength( vec3 v ) { return length( v ); }
#else
	float precisionSafeLength( vec3 v ) {
		float maxComponent = max3( abs( v ) );
		return length( v / maxComponent ) * maxComponent;
	}
#endif
struct IncidentLight {
	vec3 color;
	vec3 direction;
	bool visible;
};
struct ReflectedLight {
	vec3 directDiffuse;
	vec3 directSpecular;
	vec3 indirectDiffuse;
	vec3 indirectSpecular;
};
#ifdef USE_ALPHAHASH
	varying vec3 vPosition;
#endif
vec3 transformDirection( in vec3 dir, in mat4 matrix ) {
	return normalize( ( matrix * vec4( dir, 0.0 ) ).xyz );
}
vec3 inverseTransformDirection( in vec3 dir, in mat4 matrix ) {
	return normalize( ( vec4( dir, 0.0 ) * matrix ).xyz );
}
bool isPerspectiveMatrix( mat4 m ) {
	return m[ 2 ][ 3 ] == - 1.0;
}
vec2 equirectUv( in vec3 dir ) {
	float u = atan( dir.z, dir.x ) * RECIPROCAL_PI2 + 0.5;
	float v = asin( clamp( dir.y, - 1.0, 1.0 ) ) * RECIPROCAL_PI + 0.5;
	return vec2( u, v );
}
vec3 BRDF_Lambert( const in vec3 diffuseColor ) {
	return RECIPROCAL_PI * diffuseColor;
}
vec3 F_Schlick( const in vec3 f0, const in float f90, const in float dotVH ) {
	float fresnel = exp2( ( - 5.55473 * dotVH - 6.98316 ) * dotVH );
	return f0 * ( 1.0 - fresnel ) + ( f90 * fresnel );
}
float F_Schlick( const in float f0, const in float f90, const in float dotVH ) {
	float fresnel = exp2( ( - 5.55473 * dotVH - 6.98316 ) * dotVH );
	return f0 * ( 1.0 - fresnel ) + ( f90 * fresnel );
} // validated`,iT=`#ifdef ENVMAP_TYPE_CUBE_UV
	#define cubeUV_minMipLevel 4.0
	#define cubeUV_minTileSize 16.0
	float getFace( vec3 direction ) {
		vec3 absDirection = abs( direction );
		float face = - 1.0;
		if ( absDirection.x > absDirection.z ) {
			if ( absDirection.x > absDirection.y )
				face = direction.x > 0.0 ? 0.0 : 3.0;
			else
				face = direction.y > 0.0 ? 1.0 : 4.0;
		} else {
			if ( absDirection.z > absDirection.y )
				face = direction.z > 0.0 ? 2.0 : 5.0;
			else
				face = direction.y > 0.0 ? 1.0 : 4.0;
		}
		return face;
	}
	vec2 getUV( vec3 direction, float face ) {
		vec2 uv;
		if ( face == 0.0 ) {
			uv = vec2( direction.z, direction.y ) / abs( direction.x );
		} else if ( face == 1.0 ) {
			uv = vec2( - direction.x, - direction.z ) / abs( direction.y );
		} else if ( face == 2.0 ) {
			uv = vec2( - direction.x, direction.y ) / abs( direction.z );
		} else if ( face == 3.0 ) {
			uv = vec2( - direction.z, direction.y ) / abs( direction.x );
		} else if ( face == 4.0 ) {
			uv = vec2( - direction.x, direction.z ) / abs( direction.y );
		} else {
			uv = vec2( direction.x, direction.y ) / abs( direction.z );
		}
		return 0.5 * ( uv + 1.0 );
	}
	vec3 bilinearCubeUV( sampler2D envMap, vec3 direction, float mipInt ) {
		float face = getFace( direction );
		float filterInt = max( cubeUV_minMipLevel - mipInt, 0.0 );
		mipInt = max( mipInt, cubeUV_minMipLevel );
		float faceSize = exp2( mipInt );
		highp vec2 uv = getUV( direction, face ) * ( faceSize - 2.0 ) + 1.0;
		if ( face > 2.0 ) {
			uv.y += faceSize;
			face -= 3.0;
		}
		uv.x += face * faceSize;
		uv.x += filterInt * 3.0 * cubeUV_minTileSize;
		uv.y += 4.0 * ( exp2( CUBEUV_MAX_MIP ) - faceSize );
		uv.x *= CUBEUV_TEXEL_WIDTH;
		uv.y *= CUBEUV_TEXEL_HEIGHT;
		#ifdef texture2DGradEXT
			return texture2DGradEXT( envMap, uv, vec2( 0.0 ), vec2( 0.0 ) ).rgb;
		#else
			return texture2D( envMap, uv ).rgb;
		#endif
	}
	#define cubeUV_r0 1.0
	#define cubeUV_m0 - 2.0
	#define cubeUV_r1 0.8
	#define cubeUV_m1 - 1.0
	#define cubeUV_r4 0.4
	#define cubeUV_m4 2.0
	#define cubeUV_r5 0.305
	#define cubeUV_m5 3.0
	#define cubeUV_r6 0.21
	#define cubeUV_m6 4.0
	float roughnessToMip( float roughness ) {
		float mip = 0.0;
		if ( roughness >= cubeUV_r1 ) {
			mip = ( cubeUV_r0 - roughness ) * ( cubeUV_m1 - cubeUV_m0 ) / ( cubeUV_r0 - cubeUV_r1 ) + cubeUV_m0;
		} else if ( roughness >= cubeUV_r4 ) {
			mip = ( cubeUV_r1 - roughness ) * ( cubeUV_m4 - cubeUV_m1 ) / ( cubeUV_r1 - cubeUV_r4 ) + cubeUV_m1;
		} else if ( roughness >= cubeUV_r5 ) {
			mip = ( cubeUV_r4 - roughness ) * ( cubeUV_m5 - cubeUV_m4 ) / ( cubeUV_r4 - cubeUV_r5 ) + cubeUV_m4;
		} else if ( roughness >= cubeUV_r6 ) {
			mip = ( cubeUV_r5 - roughness ) * ( cubeUV_m6 - cubeUV_m5 ) / ( cubeUV_r5 - cubeUV_r6 ) + cubeUV_m5;
		} else {
			mip = - 2.0 * log2( 1.16 * roughness );		}
		return mip;
	}
	vec4 textureCubeUV( sampler2D envMap, vec3 sampleDir, float roughness ) {
		float mip = clamp( roughnessToMip( roughness ), cubeUV_m0, CUBEUV_MAX_MIP );
		float mipF = fract( mip );
		float mipInt = floor( mip );
		vec3 color0 = bilinearCubeUV( envMap, sampleDir, mipInt );
		if ( mipF == 0.0 ) {
			return vec4( color0, 1.0 );
		} else {
			vec3 color1 = bilinearCubeUV( envMap, sampleDir, mipInt + 1.0 );
			return vec4( mix( color0, color1, mipF ), 1.0 );
		}
	}
#endif`,aT=`vec3 transformedNormal = objectNormal;
#ifdef USE_TANGENT
	vec3 transformedTangent = objectTangent;
#endif
#ifdef USE_BATCHING
	mat3 bm = mat3( batchingMatrix );
	transformedNormal /= vec3( dot( bm[ 0 ], bm[ 0 ] ), dot( bm[ 1 ], bm[ 1 ] ), dot( bm[ 2 ], bm[ 2 ] ) );
	transformedNormal = bm * transformedNormal;
	#ifdef USE_TANGENT
		transformedTangent = bm * transformedTangent;
	#endif
#endif
#ifdef USE_INSTANCING
	mat3 im = mat3( instanceMatrix );
	transformedNormal /= vec3( dot( im[ 0 ], im[ 0 ] ), dot( im[ 1 ], im[ 1 ] ), dot( im[ 2 ], im[ 2 ] ) );
	transformedNormal = im * transformedNormal;
	#ifdef USE_TANGENT
		transformedTangent = im * transformedTangent;
	#endif
#endif
transformedNormal = normalMatrix * transformedNormal;
#ifdef FLIP_SIDED
	transformedNormal = - transformedNormal;
#endif
#ifdef USE_TANGENT
	transformedTangent = ( modelViewMatrix * vec4( transformedTangent, 0.0 ) ).xyz;
	#ifdef FLIP_SIDED
		transformedTangent = - transformedTangent;
	#endif
#endif`,sT=`#ifdef USE_DISPLACEMENTMAP
	uniform sampler2D displacementMap;
	uniform float displacementScale;
	uniform float displacementBias;
#endif`,rT=`#ifdef USE_DISPLACEMENTMAP
	transformed += normalize( objectNormal ) * ( texture2D( displacementMap, vDisplacementMapUv ).x * displacementScale + displacementBias );
#endif`,oT=`#ifdef USE_EMISSIVEMAP
	vec4 emissiveColor = texture2D( emissiveMap, vEmissiveMapUv );
	#ifdef DECODE_VIDEO_TEXTURE_EMISSIVE
		emissiveColor = sRGBTransferEOTF( emissiveColor );
	#endif
	totalEmissiveRadiance *= emissiveColor.rgb;
#endif`,lT=`#ifdef USE_EMISSIVEMAP
	uniform sampler2D emissiveMap;
#endif`,cT="gl_FragColor = linearToOutputTexel( gl_FragColor );",uT=`vec4 LinearTransferOETF( in vec4 value ) {
	return value;
}
vec4 sRGBTransferEOTF( in vec4 value ) {
	return vec4( mix( pow( value.rgb * 0.9478672986 + vec3( 0.0521327014 ), vec3( 2.4 ) ), value.rgb * 0.0773993808, vec3( lessThanEqual( value.rgb, vec3( 0.04045 ) ) ) ), value.a );
}
vec4 sRGBTransferOETF( in vec4 value ) {
	return vec4( mix( pow( value.rgb, vec3( 0.41666 ) ) * 1.055 - vec3( 0.055 ), value.rgb * 12.92, vec3( lessThanEqual( value.rgb, vec3( 0.0031308 ) ) ) ), value.a );
}`,fT=`#ifdef USE_ENVMAP
	#ifdef ENV_WORLDPOS
		vec3 cameraToFrag;
		if ( isOrthographic ) {
			cameraToFrag = normalize( vec3( - viewMatrix[ 0 ][ 2 ], - viewMatrix[ 1 ][ 2 ], - viewMatrix[ 2 ][ 2 ] ) );
		} else {
			cameraToFrag = normalize( vWorldPosition - cameraPosition );
		}
		vec3 worldNormal = inverseTransformDirection( normal, viewMatrix );
		#ifdef ENVMAP_MODE_REFLECTION
			vec3 reflectVec = reflect( cameraToFrag, worldNormal );
		#else
			vec3 reflectVec = refract( cameraToFrag, worldNormal, refractionRatio );
		#endif
	#else
		vec3 reflectVec = vReflect;
	#endif
	#ifdef ENVMAP_TYPE_CUBE
		vec4 envColor = textureCube( envMap, envMapRotation * reflectVec );
		#ifdef ENVMAP_BLENDING_MULTIPLY
			outgoingLight = mix( outgoingLight, outgoingLight * envColor.xyz, specularStrength * reflectivity );
		#elif defined( ENVMAP_BLENDING_MIX )
			outgoingLight = mix( outgoingLight, envColor.xyz, specularStrength * reflectivity );
		#elif defined( ENVMAP_BLENDING_ADD )
			outgoingLight += envColor.xyz * specularStrength * reflectivity;
		#endif
	#endif
#endif`,dT=`#ifdef USE_ENVMAP
	uniform float envMapIntensity;
	uniform mat3 envMapRotation;
	#ifdef ENVMAP_TYPE_CUBE
		uniform samplerCube envMap;
	#else
		uniform sampler2D envMap;
	#endif
#endif`,hT=`#ifdef USE_ENVMAP
	uniform float reflectivity;
	#if defined( USE_BUMPMAP ) || defined( USE_NORMALMAP ) || defined( PHONG ) || defined( LAMBERT )
		#define ENV_WORLDPOS
	#endif
	#ifdef ENV_WORLDPOS
		varying vec3 vWorldPosition;
		uniform float refractionRatio;
	#else
		varying vec3 vReflect;
	#endif
#endif`,pT=`#ifdef USE_ENVMAP
	#if defined( USE_BUMPMAP ) || defined( USE_NORMALMAP ) || defined( PHONG ) || defined( LAMBERT )
		#define ENV_WORLDPOS
	#endif
	#ifdef ENV_WORLDPOS
		
		varying vec3 vWorldPosition;
	#else
		varying vec3 vReflect;
		uniform float refractionRatio;
	#endif
#endif`,mT=`#ifdef USE_ENVMAP
	#ifdef ENV_WORLDPOS
		vWorldPosition = worldPosition.xyz;
	#else
		vec3 cameraToVertex;
		if ( isOrthographic ) {
			cameraToVertex = normalize( vec3( - viewMatrix[ 0 ][ 2 ], - viewMatrix[ 1 ][ 2 ], - viewMatrix[ 2 ][ 2 ] ) );
		} else {
			cameraToVertex = normalize( worldPosition.xyz - cameraPosition );
		}
		vec3 worldNormal = inverseTransformDirection( transformedNormal, viewMatrix );
		#ifdef ENVMAP_MODE_REFLECTION
			vReflect = reflect( cameraToVertex, worldNormal );
		#else
			vReflect = refract( cameraToVertex, worldNormal, refractionRatio );
		#endif
	#endif
#endif`,gT=`#ifdef USE_FOG
	vFogDepth = - mvPosition.z;
#endif`,_T=`#ifdef USE_FOG
	varying float vFogDepth;
#endif`,vT=`#ifdef USE_FOG
	#ifdef FOG_EXP2
		float fogFactor = 1.0 - exp( - fogDensity * fogDensity * vFogDepth * vFogDepth );
	#else
		float fogFactor = smoothstep( fogNear, fogFar, vFogDepth );
	#endif
	gl_FragColor.rgb = mix( gl_FragColor.rgb, fogColor, fogFactor );
#endif`,xT=`#ifdef USE_FOG
	uniform vec3 fogColor;
	varying float vFogDepth;
	#ifdef FOG_EXP2
		uniform float fogDensity;
	#else
		uniform float fogNear;
		uniform float fogFar;
	#endif
#endif`,yT=`#ifdef USE_GRADIENTMAP
	uniform sampler2D gradientMap;
#endif
vec3 getGradientIrradiance( vec3 normal, vec3 lightDirection ) {
	float dotNL = dot( normal, lightDirection );
	vec2 coord = vec2( dotNL * 0.5 + 0.5, 0.0 );
	#ifdef USE_GRADIENTMAP
		return vec3( texture2D( gradientMap, coord ).r );
	#else
		vec2 fw = fwidth( coord ) * 0.5;
		return mix( vec3( 0.7 ), vec3( 1.0 ), smoothstep( 0.7 - fw.x, 0.7 + fw.x, coord.x ) );
	#endif
}`,ST=`#ifdef USE_LIGHTMAP
	uniform sampler2D lightMap;
	uniform float lightMapIntensity;
#endif`,MT=`LambertMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.specularStrength = specularStrength;`,bT=`varying vec3 vViewPosition;
struct LambertMaterial {
	vec3 diffuseColor;
	float specularStrength;
};
void RE_Direct_Lambert( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in LambertMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
void RE_IndirectDiffuse_Lambert( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in LambertMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_Lambert
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Lambert`,ET=`uniform bool receiveShadow;
uniform vec3 ambientLightColor;
#if defined( USE_LIGHT_PROBES )
	uniform vec3 lightProbe[ 9 ];
#endif
vec3 shGetIrradianceAt( in vec3 normal, in vec3 shCoefficients[ 9 ] ) {
	float x = normal.x, y = normal.y, z = normal.z;
	vec3 result = shCoefficients[ 0 ] * 0.886227;
	result += shCoefficients[ 1 ] * 2.0 * 0.511664 * y;
	result += shCoefficients[ 2 ] * 2.0 * 0.511664 * z;
	result += shCoefficients[ 3 ] * 2.0 * 0.511664 * x;
	result += shCoefficients[ 4 ] * 2.0 * 0.429043 * x * y;
	result += shCoefficients[ 5 ] * 2.0 * 0.429043 * y * z;
	result += shCoefficients[ 6 ] * ( 0.743125 * z * z - 0.247708 );
	result += shCoefficients[ 7 ] * 2.0 * 0.429043 * x * z;
	result += shCoefficients[ 8 ] * 0.429043 * ( x * x - y * y );
	return result;
}
vec3 getLightProbeIrradiance( const in vec3 lightProbe[ 9 ], const in vec3 normal ) {
	vec3 worldNormal = inverseTransformDirection( normal, viewMatrix );
	vec3 irradiance = shGetIrradianceAt( worldNormal, lightProbe );
	return irradiance;
}
vec3 getAmbientLightIrradiance( const in vec3 ambientLightColor ) {
	vec3 irradiance = ambientLightColor;
	return irradiance;
}
float getDistanceAttenuation( const in float lightDistance, const in float cutoffDistance, const in float decayExponent ) {
	float distanceFalloff = 1.0 / max( pow( lightDistance, decayExponent ), 0.01 );
	if ( cutoffDistance > 0.0 ) {
		distanceFalloff *= pow2( saturate( 1.0 - pow4( lightDistance / cutoffDistance ) ) );
	}
	return distanceFalloff;
}
float getSpotAttenuation( const in float coneCosine, const in float penumbraCosine, const in float angleCosine ) {
	return smoothstep( coneCosine, penumbraCosine, angleCosine );
}
#if NUM_DIR_LIGHTS > 0
	struct DirectionalLight {
		vec3 direction;
		vec3 color;
	};
	uniform DirectionalLight directionalLights[ NUM_DIR_LIGHTS ];
	void getDirectionalLightInfo( const in DirectionalLight directionalLight, out IncidentLight light ) {
		light.color = directionalLight.color;
		light.direction = directionalLight.direction;
		light.visible = true;
	}
#endif
#if NUM_POINT_LIGHTS > 0
	struct PointLight {
		vec3 position;
		vec3 color;
		float distance;
		float decay;
	};
	uniform PointLight pointLights[ NUM_POINT_LIGHTS ];
	void getPointLightInfo( const in PointLight pointLight, const in vec3 geometryPosition, out IncidentLight light ) {
		vec3 lVector = pointLight.position - geometryPosition;
		light.direction = normalize( lVector );
		float lightDistance = length( lVector );
		light.color = pointLight.color;
		light.color *= getDistanceAttenuation( lightDistance, pointLight.distance, pointLight.decay );
		light.visible = ( light.color != vec3( 0.0 ) );
	}
#endif
#if NUM_SPOT_LIGHTS > 0
	struct SpotLight {
		vec3 position;
		vec3 direction;
		vec3 color;
		float distance;
		float decay;
		float coneCos;
		float penumbraCos;
	};
	uniform SpotLight spotLights[ NUM_SPOT_LIGHTS ];
	void getSpotLightInfo( const in SpotLight spotLight, const in vec3 geometryPosition, out IncidentLight light ) {
		vec3 lVector = spotLight.position - geometryPosition;
		light.direction = normalize( lVector );
		float angleCos = dot( light.direction, spotLight.direction );
		float spotAttenuation = getSpotAttenuation( spotLight.coneCos, spotLight.penumbraCos, angleCos );
		if ( spotAttenuation > 0.0 ) {
			float lightDistance = length( lVector );
			light.color = spotLight.color * spotAttenuation;
			light.color *= getDistanceAttenuation( lightDistance, spotLight.distance, spotLight.decay );
			light.visible = ( light.color != vec3( 0.0 ) );
		} else {
			light.color = vec3( 0.0 );
			light.visible = false;
		}
	}
#endif
#if NUM_RECT_AREA_LIGHTS > 0
	struct RectAreaLight {
		vec3 color;
		vec3 position;
		vec3 halfWidth;
		vec3 halfHeight;
	};
	uniform sampler2D ltc_1;	uniform sampler2D ltc_2;
	uniform RectAreaLight rectAreaLights[ NUM_RECT_AREA_LIGHTS ];
#endif
#if NUM_HEMI_LIGHTS > 0
	struct HemisphereLight {
		vec3 direction;
		vec3 skyColor;
		vec3 groundColor;
	};
	uniform HemisphereLight hemisphereLights[ NUM_HEMI_LIGHTS ];
	vec3 getHemisphereLightIrradiance( const in HemisphereLight hemiLight, const in vec3 normal ) {
		float dotNL = dot( normal, hemiLight.direction );
		float hemiDiffuseWeight = 0.5 * dotNL + 0.5;
		vec3 irradiance = mix( hemiLight.groundColor, hemiLight.skyColor, hemiDiffuseWeight );
		return irradiance;
	}
#endif
#include <lightprobes_pars_fragment>`,TT=`#ifdef USE_ENVMAP
	vec3 getIBLIrradiance( const in vec3 normal ) {
		#ifdef ENVMAP_TYPE_CUBE_UV
			vec3 worldNormal = inverseTransformDirection( normal, viewMatrix );
			vec4 envMapColor = textureCubeUV( envMap, envMapRotation * worldNormal, 1.0 );
			return PI * envMapColor.rgb * envMapIntensity;
		#else
			return vec3( 0.0 );
		#endif
	}
	vec3 getIBLRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness ) {
		#ifdef ENVMAP_TYPE_CUBE_UV
			vec3 reflectVec = reflect( - viewDir, normal );
			reflectVec = normalize( mix( reflectVec, normal, pow4( roughness ) ) );
			reflectVec = inverseTransformDirection( reflectVec, viewMatrix );
			vec4 envMapColor = textureCubeUV( envMap, envMapRotation * reflectVec, roughness );
			return envMapColor.rgb * envMapIntensity;
		#else
			return vec3( 0.0 );
		#endif
	}
	#ifdef USE_ANISOTROPY
		vec3 getIBLAnisotropyRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness, const in vec3 bitangent, const in float anisotropy ) {
			#ifdef ENVMAP_TYPE_CUBE_UV
				vec3 bentNormal = cross( bitangent, viewDir );
				bentNormal = normalize( cross( bentNormal, bitangent ) );
				bentNormal = normalize( mix( bentNormal, normal, pow2( pow2( 1.0 - anisotropy * ( 1.0 - roughness ) ) ) ) );
				return getIBLRadiance( viewDir, bentNormal, roughness );
			#else
				return vec3( 0.0 );
			#endif
		}
	#endif
#endif`,AT=`ToonMaterial material;
material.diffuseColor = diffuseColor.rgb;`,wT=`varying vec3 vViewPosition;
struct ToonMaterial {
	vec3 diffuseColor;
};
void RE_Direct_Toon( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in ToonMaterial material, inout ReflectedLight reflectedLight ) {
	vec3 irradiance = getGradientIrradiance( geometryNormal, directLight.direction ) * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
void RE_IndirectDiffuse_Toon( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in ToonMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_Toon
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Toon`,RT=`BlinnPhongMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.specularColor = specular;
material.specularShininess = shininess;
material.specularStrength = specularStrength;`,CT=`varying vec3 vViewPosition;
struct BlinnPhongMaterial {
	vec3 diffuseColor;
	vec3 specularColor;
	float specularShininess;
	float specularStrength;
};
void RE_Direct_BlinnPhong( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
	reflectedLight.directSpecular += irradiance * BRDF_BlinnPhong( directLight.direction, geometryViewDir, geometryNormal, material.specularColor, material.specularShininess ) * material.specularStrength;
}
void RE_IndirectDiffuse_BlinnPhong( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_BlinnPhong
#define RE_IndirectDiffuse		RE_IndirectDiffuse_BlinnPhong`,NT=`PhysicalMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.diffuseContribution = diffuseColor.rgb * ( 1.0 - metalnessFactor );
material.metalness = metalnessFactor;
vec3 dxy = max( abs( dFdx( nonPerturbedNormal ) ), abs( dFdy( nonPerturbedNormal ) ) );
float geometryRoughness = max( max( dxy.x, dxy.y ), dxy.z );
material.roughness = max( roughnessFactor, 0.0525 );material.roughness += geometryRoughness;
material.roughness = min( material.roughness, 1.0 );
#ifdef IOR
	material.ior = ior;
	#ifdef USE_SPECULAR
		float specularIntensityFactor = specularIntensity;
		vec3 specularColorFactor = specularColor;
		#ifdef USE_SPECULAR_COLORMAP
			specularColorFactor *= texture2D( specularColorMap, vSpecularColorMapUv ).rgb;
		#endif
		#ifdef USE_SPECULAR_INTENSITYMAP
			specularIntensityFactor *= texture2D( specularIntensityMap, vSpecularIntensityMapUv ).a;
		#endif
		material.specularF90 = mix( specularIntensityFactor, 1.0, metalnessFactor );
	#else
		float specularIntensityFactor = 1.0;
		vec3 specularColorFactor = vec3( 1.0 );
		material.specularF90 = 1.0;
	#endif
	material.specularColor = min( pow2( ( material.ior - 1.0 ) / ( material.ior + 1.0 ) ) * specularColorFactor, vec3( 1.0 ) ) * specularIntensityFactor;
	material.specularColorBlended = mix( material.specularColor, diffuseColor.rgb, metalnessFactor );
#else
	material.specularColor = vec3( 0.04 );
	material.specularColorBlended = mix( material.specularColor, diffuseColor.rgb, metalnessFactor );
	material.specularF90 = 1.0;
#endif
#ifdef USE_CLEARCOAT
	material.clearcoat = clearcoat;
	material.clearcoatRoughness = clearcoatRoughness;
	material.clearcoatF0 = vec3( 0.04 );
	material.clearcoatF90 = 1.0;
	#ifdef USE_CLEARCOATMAP
		material.clearcoat *= texture2D( clearcoatMap, vClearcoatMapUv ).x;
	#endif
	#ifdef USE_CLEARCOAT_ROUGHNESSMAP
		material.clearcoatRoughness *= texture2D( clearcoatRoughnessMap, vClearcoatRoughnessMapUv ).y;
	#endif
	material.clearcoat = saturate( material.clearcoat );	material.clearcoatRoughness = max( material.clearcoatRoughness, 0.0525 );
	material.clearcoatRoughness += geometryRoughness;
	material.clearcoatRoughness = min( material.clearcoatRoughness, 1.0 );
#endif
#ifdef USE_DISPERSION
	material.dispersion = dispersion;
#endif
#ifdef USE_IRIDESCENCE
	material.iridescence = iridescence;
	material.iridescenceIOR = iridescenceIOR;
	#ifdef USE_IRIDESCENCEMAP
		material.iridescence *= texture2D( iridescenceMap, vIridescenceMapUv ).r;
	#endif
	#ifdef USE_IRIDESCENCE_THICKNESSMAP
		material.iridescenceThickness = (iridescenceThicknessMaximum - iridescenceThicknessMinimum) * texture2D( iridescenceThicknessMap, vIridescenceThicknessMapUv ).g + iridescenceThicknessMinimum;
	#else
		material.iridescenceThickness = iridescenceThicknessMaximum;
	#endif
#endif
#ifdef USE_SHEEN
	material.sheenColor = sheenColor;
	#ifdef USE_SHEEN_COLORMAP
		material.sheenColor *= texture2D( sheenColorMap, vSheenColorMapUv ).rgb;
	#endif
	material.sheenRoughness = clamp( sheenRoughness, 0.0001, 1.0 );
	#ifdef USE_SHEEN_ROUGHNESSMAP
		material.sheenRoughness *= texture2D( sheenRoughnessMap, vSheenRoughnessMapUv ).a;
	#endif
#endif
#ifdef USE_ANISOTROPY
	#ifdef USE_ANISOTROPYMAP
		mat2 anisotropyMat = mat2( anisotropyVector.x, anisotropyVector.y, - anisotropyVector.y, anisotropyVector.x );
		vec3 anisotropyPolar = texture2D( anisotropyMap, vAnisotropyMapUv ).rgb;
		vec2 anisotropyV = anisotropyMat * normalize( 2.0 * anisotropyPolar.rg - vec2( 1.0 ) ) * anisotropyPolar.b;
	#else
		vec2 anisotropyV = anisotropyVector;
	#endif
	material.anisotropy = length( anisotropyV );
	if( material.anisotropy == 0.0 ) {
		anisotropyV = vec2( 1.0, 0.0 );
	} else {
		anisotropyV /= material.anisotropy;
		material.anisotropy = saturate( material.anisotropy );
	}
	material.alphaT = mix( pow2( material.roughness ), 1.0, pow2( material.anisotropy ) );
	material.anisotropyT = tbn[ 0 ] * anisotropyV.x + tbn[ 1 ] * anisotropyV.y;
	material.anisotropyB = tbn[ 1 ] * anisotropyV.x - tbn[ 0 ] * anisotropyV.y;
#endif`,DT=`uniform sampler2D dfgLUT;
struct PhysicalMaterial {
	vec3 diffuseColor;
	vec3 diffuseContribution;
	vec3 specularColor;
	vec3 specularColorBlended;
	float roughness;
	float metalness;
	float specularF90;
	float dispersion;
	#ifdef USE_CLEARCOAT
		float clearcoat;
		float clearcoatRoughness;
		vec3 clearcoatF0;
		float clearcoatF90;
	#endif
	#ifdef USE_IRIDESCENCE
		float iridescence;
		float iridescenceIOR;
		float iridescenceThickness;
		vec3 iridescenceFresnel;
		vec3 iridescenceF0;
		vec3 iridescenceFresnelDielectric;
		vec3 iridescenceFresnelMetallic;
	#endif
	#ifdef USE_SHEEN
		vec3 sheenColor;
		float sheenRoughness;
	#endif
	#ifdef IOR
		float ior;
	#endif
	#ifdef USE_TRANSMISSION
		float transmission;
		float transmissionAlpha;
		float thickness;
		float attenuationDistance;
		vec3 attenuationColor;
	#endif
	#ifdef USE_ANISOTROPY
		float anisotropy;
		float alphaT;
		vec3 anisotropyT;
		vec3 anisotropyB;
	#endif
};
vec3 clearcoatSpecularDirect = vec3( 0.0 );
vec3 clearcoatSpecularIndirect = vec3( 0.0 );
vec3 sheenSpecularDirect = vec3( 0.0 );
vec3 sheenSpecularIndirect = vec3(0.0 );
vec3 Schlick_to_F0( const in vec3 f, const in float f90, const in float dotVH ) {
    float x = clamp( 1.0 - dotVH, 0.0, 1.0 );
    float x2 = x * x;
    float x5 = clamp( x * x2 * x2, 0.0, 0.9999 );
    return ( f - vec3( f90 ) * x5 ) / ( 1.0 - x5 );
}
float V_GGX_SmithCorrelated( const in float alpha, const in float dotNL, const in float dotNV ) {
	float a2 = pow2( alpha );
	float gv = dotNL * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNV ) );
	float gl = dotNV * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNL ) );
	return 0.5 / max( gv + gl, EPSILON );
}
float D_GGX( const in float alpha, const in float dotNH ) {
	float a2 = pow2( alpha );
	float denom = pow2( dotNH ) * ( a2 - 1.0 ) + 1.0;
	return RECIPROCAL_PI * a2 / pow2( denom );
}
#ifdef USE_ANISOTROPY
	float V_GGX_SmithCorrelated_Anisotropic( const in float alphaT, const in float alphaB, const in float dotTV, const in float dotBV, const in float dotTL, const in float dotBL, const in float dotNV, const in float dotNL ) {
		float gv = dotNL * length( vec3( alphaT * dotTV, alphaB * dotBV, dotNV ) );
		float gl = dotNV * length( vec3( alphaT * dotTL, alphaB * dotBL, dotNL ) );
		return 0.5 / max( gv + gl, EPSILON );
	}
	float D_GGX_Anisotropic( const in float alphaT, const in float alphaB, const in float dotNH, const in float dotTH, const in float dotBH ) {
		float a2 = alphaT * alphaB;
		highp vec3 v = vec3( alphaB * dotTH, alphaT * dotBH, a2 * dotNH );
		highp float v2 = dot( v, v );
		float w2 = a2 / v2;
		return RECIPROCAL_PI * a2 * pow2 ( w2 );
	}
#endif
#ifdef USE_CLEARCOAT
	vec3 BRDF_GGX_Clearcoat( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in PhysicalMaterial material) {
		vec3 f0 = material.clearcoatF0;
		float f90 = material.clearcoatF90;
		float roughness = material.clearcoatRoughness;
		float alpha = pow2( roughness );
		vec3 halfDir = normalize( lightDir + viewDir );
		float dotNL = saturate( dot( normal, lightDir ) );
		float dotNV = saturate( dot( normal, viewDir ) );
		float dotNH = saturate( dot( normal, halfDir ) );
		float dotVH = saturate( dot( viewDir, halfDir ) );
		vec3 F = F_Schlick( f0, f90, dotVH );
		float V = V_GGX_SmithCorrelated( alpha, dotNL, dotNV );
		float D = D_GGX( alpha, dotNH );
		return F * ( V * D );
	}
#endif
vec3 BRDF_GGX( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in PhysicalMaterial material ) {
	vec3 f0 = material.specularColorBlended;
	float f90 = material.specularF90;
	float roughness = material.roughness;
	float alpha = pow2( roughness );
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNL = saturate( dot( normal, lightDir ) );
	float dotNV = saturate( dot( normal, viewDir ) );
	float dotNH = saturate( dot( normal, halfDir ) );
	float dotVH = saturate( dot( viewDir, halfDir ) );
	vec3 F = F_Schlick( f0, f90, dotVH );
	#ifdef USE_IRIDESCENCE
		F = mix( F, material.iridescenceFresnel, material.iridescence );
	#endif
	#ifdef USE_ANISOTROPY
		float dotTL = dot( material.anisotropyT, lightDir );
		float dotTV = dot( material.anisotropyT, viewDir );
		float dotTH = dot( material.anisotropyT, halfDir );
		float dotBL = dot( material.anisotropyB, lightDir );
		float dotBV = dot( material.anisotropyB, viewDir );
		float dotBH = dot( material.anisotropyB, halfDir );
		float V = V_GGX_SmithCorrelated_Anisotropic( material.alphaT, alpha, dotTV, dotBV, dotTL, dotBL, dotNV, dotNL );
		float D = D_GGX_Anisotropic( material.alphaT, alpha, dotNH, dotTH, dotBH );
	#else
		float V = V_GGX_SmithCorrelated( alpha, dotNL, dotNV );
		float D = D_GGX( alpha, dotNH );
	#endif
	return F * ( V * D );
}
vec2 LTC_Uv( const in vec3 N, const in vec3 V, const in float roughness ) {
	const float LUT_SIZE = 64.0;
	const float LUT_SCALE = ( LUT_SIZE - 1.0 ) / LUT_SIZE;
	const float LUT_BIAS = 0.5 / LUT_SIZE;
	float dotNV = saturate( dot( N, V ) );
	vec2 uv = vec2( roughness, sqrt( 1.0 - dotNV ) );
	uv = uv * LUT_SCALE + LUT_BIAS;
	return uv;
}
float LTC_ClippedSphereFormFactor( const in vec3 f ) {
	float l = length( f );
	return max( ( l * l + f.z ) / ( l + 1.0 ), 0.0 );
}
vec3 LTC_EdgeVectorFormFactor( const in vec3 v1, const in vec3 v2 ) {
	float x = dot( v1, v2 );
	float y = abs( x );
	float a = 0.8543985 + ( 0.4965155 + 0.0145206 * y ) * y;
	float b = 3.4175940 + ( 4.1616724 + y ) * y;
	float v = a / b;
	float theta_sintheta = ( x > 0.0 ) ? v : 0.5 * inversesqrt( max( 1.0 - x * x, 1e-7 ) ) - v;
	return cross( v1, v2 ) * theta_sintheta;
}
vec3 LTC_Evaluate( const in vec3 N, const in vec3 V, const in vec3 P, const in mat3 mInv, const in vec3 rectCoords[ 4 ] ) {
	vec3 v1 = rectCoords[ 1 ] - rectCoords[ 0 ];
	vec3 v2 = rectCoords[ 3 ] - rectCoords[ 0 ];
	vec3 lightNormal = cross( v1, v2 );
	if( dot( lightNormal, P - rectCoords[ 0 ] ) < 0.0 ) return vec3( 0.0 );
	vec3 T1, T2;
	T1 = normalize( V - N * dot( V, N ) );
	T2 = - cross( N, T1 );
	mat3 mat = mInv * transpose( mat3( T1, T2, N ) );
	vec3 coords[ 4 ];
	coords[ 0 ] = mat * ( rectCoords[ 0 ] - P );
	coords[ 1 ] = mat * ( rectCoords[ 1 ] - P );
	coords[ 2 ] = mat * ( rectCoords[ 2 ] - P );
	coords[ 3 ] = mat * ( rectCoords[ 3 ] - P );
	coords[ 0 ] = normalize( coords[ 0 ] );
	coords[ 1 ] = normalize( coords[ 1 ] );
	coords[ 2 ] = normalize( coords[ 2 ] );
	coords[ 3 ] = normalize( coords[ 3 ] );
	vec3 vectorFormFactor = vec3( 0.0 );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 0 ], coords[ 1 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 1 ], coords[ 2 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 2 ], coords[ 3 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 3 ], coords[ 0 ] );
	float result = LTC_ClippedSphereFormFactor( vectorFormFactor );
	return vec3( result );
}
#if defined( USE_SHEEN )
float D_Charlie( float roughness, float dotNH ) {
	float alpha = pow2( roughness );
	float invAlpha = 1.0 / alpha;
	float cos2h = dotNH * dotNH;
	float sin2h = max( 1.0 - cos2h, 0.0078125 );
	return ( 2.0 + invAlpha ) * pow( sin2h, invAlpha * 0.5 ) / ( 2.0 * PI );
}
float V_Neubelt( float dotNV, float dotNL ) {
	return saturate( 1.0 / ( 4.0 * ( dotNL + dotNV - dotNL * dotNV ) ) );
}
vec3 BRDF_Sheen( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, vec3 sheenColor, const in float sheenRoughness ) {
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNL = saturate( dot( normal, lightDir ) );
	float dotNV = saturate( dot( normal, viewDir ) );
	float dotNH = saturate( dot( normal, halfDir ) );
	float D = D_Charlie( sheenRoughness, dotNH );
	float V = V_Neubelt( dotNV, dotNL );
	return sheenColor * ( D * V );
}
#endif
float IBLSheenBRDF( const in vec3 normal, const in vec3 viewDir, const in float roughness ) {
	float dotNV = saturate( dot( normal, viewDir ) );
	float r2 = roughness * roughness;
	float rInv = 1.0 / ( roughness + 0.1 );
	float a = -1.9362 + 1.0678 * roughness + 0.4573 * r2 - 0.8469 * rInv;
	float b = -0.6014 + 0.5538 * roughness - 0.4670 * r2 - 0.1255 * rInv;
	float DG = exp( a * dotNV + b );
	return saturate( DG );
}
vec3 EnvironmentBRDF( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float roughness ) {
	float dotNV = saturate( dot( normal, viewDir ) );
	vec2 fab = texture2D( dfgLUT, vec2( roughness, dotNV ) ).rg;
	return specularColor * fab.x + specularF90 * fab.y;
}
#ifdef USE_IRIDESCENCE
void computeMultiscatteringIridescence( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float iridescence, const in vec3 iridescenceF0, const in float roughness, inout vec3 singleScatter, inout vec3 multiScatter ) {
#else
void computeMultiscattering( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float roughness, inout vec3 singleScatter, inout vec3 multiScatter ) {
#endif
	float dotNV = saturate( dot( normal, viewDir ) );
	vec2 fab = texture2D( dfgLUT, vec2( roughness, dotNV ) ).rg;
	#ifdef USE_IRIDESCENCE
		vec3 Fr = mix( specularColor, iridescenceF0, iridescence );
	#else
		vec3 Fr = specularColor;
	#endif
	vec3 FssEss = Fr * fab.x + specularF90 * fab.y;
	float Ess = fab.x + fab.y;
	float Ems = 1.0 - Ess;
	vec3 Favg = Fr + ( 1.0 - Fr ) * 0.047619;	vec3 Fms = FssEss * Favg / ( 1.0 - Ems * Favg );
	singleScatter += FssEss;
	multiScatter += Fms * Ems;
}
vec3 BRDF_GGX_Multiscatter( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in PhysicalMaterial material ) {
	vec3 singleScatter = BRDF_GGX( lightDir, viewDir, normal, material );
	float dotNL = saturate( dot( normal, lightDir ) );
	float dotNV = saturate( dot( normal, viewDir ) );
	vec2 dfgV = texture2D( dfgLUT, vec2( material.roughness, dotNV ) ).rg;
	vec2 dfgL = texture2D( dfgLUT, vec2( material.roughness, dotNL ) ).rg;
	vec3 FssEss_V = material.specularColorBlended * dfgV.x + material.specularF90 * dfgV.y;
	vec3 FssEss_L = material.specularColorBlended * dfgL.x + material.specularF90 * dfgL.y;
	float Ess_V = dfgV.x + dfgV.y;
	float Ess_L = dfgL.x + dfgL.y;
	float Ems_V = 1.0 - Ess_V;
	float Ems_L = 1.0 - Ess_L;
	vec3 Favg = material.specularColorBlended + ( 1.0 - material.specularColorBlended ) * 0.047619;
	vec3 Fms = FssEss_V * FssEss_L * Favg / ( 1.0 - Ems_V * Ems_L * Favg + EPSILON );
	float compensationFactor = Ems_V * Ems_L;
	vec3 multiScatter = Fms * compensationFactor;
	return singleScatter + multiScatter;
}
#if NUM_RECT_AREA_LIGHTS > 0
	void RE_Direct_RectArea_Physical( const in RectAreaLight rectAreaLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
		vec3 normal = geometryNormal;
		vec3 viewDir = geometryViewDir;
		vec3 position = geometryPosition;
		vec3 lightPos = rectAreaLight.position;
		vec3 halfWidth = rectAreaLight.halfWidth;
		vec3 halfHeight = rectAreaLight.halfHeight;
		vec3 lightColor = rectAreaLight.color;
		float roughness = material.roughness;
		vec3 rectCoords[ 4 ];
		rectCoords[ 0 ] = lightPos + halfWidth - halfHeight;		rectCoords[ 1 ] = lightPos - halfWidth - halfHeight;
		rectCoords[ 2 ] = lightPos - halfWidth + halfHeight;
		rectCoords[ 3 ] = lightPos + halfWidth + halfHeight;
		vec2 uv = LTC_Uv( normal, viewDir, roughness );
		vec4 t1 = texture2D( ltc_1, uv );
		vec4 t2 = texture2D( ltc_2, uv );
		mat3 mInv = mat3(
			vec3( t1.x, 0, t1.y ),
			vec3(    0, 1,    0 ),
			vec3( t1.z, 0, t1.w )
		);
		vec3 fresnel = ( material.specularColorBlended * t2.x + ( material.specularF90 - material.specularColorBlended ) * t2.y );
		reflectedLight.directSpecular += lightColor * fresnel * LTC_Evaluate( normal, viewDir, position, mInv, rectCoords );
		reflectedLight.directDiffuse += lightColor * material.diffuseContribution * LTC_Evaluate( normal, viewDir, position, mat3( 1.0 ), rectCoords );
		#ifdef USE_CLEARCOAT
			vec3 Ncc = geometryClearcoatNormal;
			vec2 uvClearcoat = LTC_Uv( Ncc, viewDir, material.clearcoatRoughness );
			vec4 t1Clearcoat = texture2D( ltc_1, uvClearcoat );
			vec4 t2Clearcoat = texture2D( ltc_2, uvClearcoat );
			mat3 mInvClearcoat = mat3(
				vec3( t1Clearcoat.x, 0, t1Clearcoat.y ),
				vec3(             0, 1,             0 ),
				vec3( t1Clearcoat.z, 0, t1Clearcoat.w )
			);
			vec3 fresnelClearcoat = material.clearcoatF0 * t2Clearcoat.x + ( material.clearcoatF90 - material.clearcoatF0 ) * t2Clearcoat.y;
			clearcoatSpecularDirect += lightColor * fresnelClearcoat * LTC_Evaluate( Ncc, viewDir, position, mInvClearcoat, rectCoords );
		#endif
	}
#endif
void RE_Direct_Physical( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	#ifdef USE_CLEARCOAT
		float dotNLcc = saturate( dot( geometryClearcoatNormal, directLight.direction ) );
		vec3 ccIrradiance = dotNLcc * directLight.color;
		clearcoatSpecularDirect += ccIrradiance * BRDF_GGX_Clearcoat( directLight.direction, geometryViewDir, geometryClearcoatNormal, material );
	#endif
	#ifdef USE_SHEEN
 
 		sheenSpecularDirect += irradiance * BRDF_Sheen( directLight.direction, geometryViewDir, geometryNormal, material.sheenColor, material.sheenRoughness );
 
 		float sheenAlbedoV = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
 		float sheenAlbedoL = IBLSheenBRDF( geometryNormal, directLight.direction, material.sheenRoughness );
 
 		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * max( sheenAlbedoV, sheenAlbedoL );
 
 		irradiance *= sheenEnergyComp;
 
 	#endif
	reflectedLight.directSpecular += irradiance * BRDF_GGX_Multiscatter( directLight.direction, geometryViewDir, geometryNormal, material );
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseContribution );
}
void RE_IndirectDiffuse_Physical( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
	vec3 diffuse = irradiance * BRDF_Lambert( material.diffuseContribution );
	#ifdef USE_SHEEN
		float sheenAlbedo = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * sheenAlbedo;
		diffuse *= sheenEnergyComp;
	#endif
	reflectedLight.indirectDiffuse += diffuse;
}
void RE_IndirectSpecular_Physical( const in vec3 radiance, const in vec3 irradiance, const in vec3 clearcoatRadiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight) {
	#ifdef USE_CLEARCOAT
		clearcoatSpecularIndirect += clearcoatRadiance * EnvironmentBRDF( geometryClearcoatNormal, geometryViewDir, material.clearcoatF0, material.clearcoatF90, material.clearcoatRoughness );
	#endif
	#ifdef USE_SHEEN
		sheenSpecularIndirect += irradiance * material.sheenColor * IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness ) * RECIPROCAL_PI;
 	#endif
	vec3 singleScatteringDielectric = vec3( 0.0 );
	vec3 multiScatteringDielectric = vec3( 0.0 );
	vec3 singleScatteringMetallic = vec3( 0.0 );
	vec3 multiScatteringMetallic = vec3( 0.0 );
	#ifdef USE_IRIDESCENCE
		computeMultiscatteringIridescence( geometryNormal, geometryViewDir, material.specularColor, material.specularF90, material.iridescence, material.iridescenceFresnelDielectric, material.roughness, singleScatteringDielectric, multiScatteringDielectric );
		computeMultiscatteringIridescence( geometryNormal, geometryViewDir, material.diffuseColor, material.specularF90, material.iridescence, material.iridescenceFresnelMetallic, material.roughness, singleScatteringMetallic, multiScatteringMetallic );
	#else
		computeMultiscattering( geometryNormal, geometryViewDir, material.specularColor, material.specularF90, material.roughness, singleScatteringDielectric, multiScatteringDielectric );
		computeMultiscattering( geometryNormal, geometryViewDir, material.diffuseColor, material.specularF90, material.roughness, singleScatteringMetallic, multiScatteringMetallic );
	#endif
	vec3 singleScattering = mix( singleScatteringDielectric, singleScatteringMetallic, material.metalness );
	vec3 multiScattering = mix( multiScatteringDielectric, multiScatteringMetallic, material.metalness );
	vec3 totalScatteringDielectric = singleScatteringDielectric + multiScatteringDielectric;
	vec3 diffuse = material.diffuseContribution * ( 1.0 - totalScatteringDielectric );
	vec3 cosineWeightedIrradiance = irradiance * RECIPROCAL_PI;
	vec3 indirectSpecular = radiance * singleScattering;
	indirectSpecular += multiScattering * cosineWeightedIrradiance;
	vec3 indirectDiffuse = diffuse * cosineWeightedIrradiance;
	#ifdef USE_SHEEN
		float sheenAlbedo = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * sheenAlbedo;
		indirectSpecular *= sheenEnergyComp;
		indirectDiffuse *= sheenEnergyComp;
	#endif
	reflectedLight.indirectSpecular += indirectSpecular;
	reflectedLight.indirectDiffuse += indirectDiffuse;
}
#define RE_Direct				RE_Direct_Physical
#define RE_Direct_RectArea		RE_Direct_RectArea_Physical
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Physical
#define RE_IndirectSpecular		RE_IndirectSpecular_Physical
float computeSpecularOcclusion( const in float dotNV, const in float ambientOcclusion, const in float roughness ) {
	return saturate( pow( dotNV + ambientOcclusion, exp2( - 16.0 * roughness - 1.0 ) ) - 1.0 + ambientOcclusion );
}`,UT=`
vec3 geometryPosition = - vViewPosition;
vec3 geometryNormal = normal;
vec3 geometryViewDir = ( isOrthographic ) ? vec3( 0, 0, 1 ) : normalize( vViewPosition );
vec3 geometryClearcoatNormal = vec3( 0.0 );
#ifdef USE_CLEARCOAT
	geometryClearcoatNormal = clearcoatNormal;
#endif
#ifdef USE_IRIDESCENCE
	float dotNVi = saturate( dot( normal, geometryViewDir ) );
	if ( material.iridescenceThickness == 0.0 ) {
		material.iridescence = 0.0;
	} else {
		material.iridescence = saturate( material.iridescence );
	}
	if ( material.iridescence > 0.0 ) {
		material.iridescenceFresnelDielectric = evalIridescence( 1.0, material.iridescenceIOR, dotNVi, material.iridescenceThickness, material.specularColor );
		material.iridescenceFresnelMetallic = evalIridescence( 1.0, material.iridescenceIOR, dotNVi, material.iridescenceThickness, material.diffuseColor );
		material.iridescenceFresnel = mix( material.iridescenceFresnelDielectric, material.iridescenceFresnelMetallic, material.metalness );
		material.iridescenceF0 = Schlick_to_F0( material.iridescenceFresnel, 1.0, dotNVi );
	}
#endif
IncidentLight directLight;
#if ( NUM_POINT_LIGHTS > 0 ) && defined( RE_Direct )
	PointLight pointLight;
	#if defined( USE_SHADOWMAP ) && NUM_POINT_LIGHT_SHADOWS > 0
	PointLightShadow pointLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_POINT_LIGHTS; i ++ ) {
		pointLight = pointLights[ i ];
		getPointLightInfo( pointLight, geometryPosition, directLight );
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_POINT_LIGHT_SHADOWS ) && ( defined( SHADOWMAP_TYPE_PCF ) || defined( SHADOWMAP_TYPE_BASIC ) )
		pointLightShadow = pointLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getPointShadow( pointShadowMap[ i ], pointLightShadow.shadowMapSize, pointLightShadow.shadowIntensity, pointLightShadow.shadowBias, pointLightShadow.shadowRadius, vPointShadowCoord[ i ], pointLightShadow.shadowCameraNear, pointLightShadow.shadowCameraFar ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_SPOT_LIGHTS > 0 ) && defined( RE_Direct )
	SpotLight spotLight;
	vec4 spotColor;
	vec3 spotLightCoord;
	bool inSpotLightMap;
	#if defined( USE_SHADOWMAP ) && NUM_SPOT_LIGHT_SHADOWS > 0
	SpotLightShadow spotLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHTS; i ++ ) {
		spotLight = spotLights[ i ];
		getSpotLightInfo( spotLight, geometryPosition, directLight );
		#if ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS )
		#define SPOT_LIGHT_MAP_INDEX UNROLLED_LOOP_INDEX
		#elif ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
		#define SPOT_LIGHT_MAP_INDEX NUM_SPOT_LIGHT_MAPS
		#else
		#define SPOT_LIGHT_MAP_INDEX ( UNROLLED_LOOP_INDEX - NUM_SPOT_LIGHT_SHADOWS + NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS )
		#endif
		#if ( SPOT_LIGHT_MAP_INDEX < NUM_SPOT_LIGHT_MAPS )
			spotLightCoord = vSpotLightCoord[ i ].xyz / vSpotLightCoord[ i ].w;
			inSpotLightMap = all( lessThan( abs( spotLightCoord * 2. - 1. ), vec3( 1.0 ) ) );
			spotColor = texture2D( spotLightMap[ SPOT_LIGHT_MAP_INDEX ], spotLightCoord.xy );
			directLight.color = inSpotLightMap ? directLight.color * spotColor.rgb : directLight.color;
		#endif
		#undef SPOT_LIGHT_MAP_INDEX
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
		spotLightShadow = spotLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getShadow( spotShadowMap[ i ], spotLightShadow.shadowMapSize, spotLightShadow.shadowIntensity, spotLightShadow.shadowBias, spotLightShadow.shadowRadius, vSpotLightCoord[ i ] ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_DIR_LIGHTS > 0 ) && defined( RE_Direct )
	DirectionalLight directionalLight;
	#if defined( USE_SHADOWMAP ) && NUM_DIR_LIGHT_SHADOWS > 0
	DirectionalLightShadow directionalLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_DIR_LIGHTS; i ++ ) {
		directionalLight = directionalLights[ i ];
		getDirectionalLightInfo( directionalLight, directLight );
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_DIR_LIGHT_SHADOWS )
		directionalLightShadow = directionalLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getShadow( directionalShadowMap[ i ], directionalLightShadow.shadowMapSize, directionalLightShadow.shadowIntensity, directionalLightShadow.shadowBias, directionalLightShadow.shadowRadius, vDirectionalShadowCoord[ i ] ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_RECT_AREA_LIGHTS > 0 ) && defined( RE_Direct_RectArea )
	RectAreaLight rectAreaLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_RECT_AREA_LIGHTS; i ++ ) {
		rectAreaLight = rectAreaLights[ i ];
		RE_Direct_RectArea( rectAreaLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if defined( RE_IndirectDiffuse )
	vec3 iblIrradiance = vec3( 0.0 );
	vec3 irradiance = getAmbientLightIrradiance( ambientLightColor );
	#if defined( USE_LIGHT_PROBES )
		irradiance += getLightProbeIrradiance( lightProbe, geometryNormal );
	#endif
	#if ( NUM_HEMI_LIGHTS > 0 )
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_HEMI_LIGHTS; i ++ ) {
			irradiance += getHemisphereLightIrradiance( hemisphereLights[ i ], geometryNormal );
		}
		#pragma unroll_loop_end
	#endif
	#ifdef USE_LIGHT_PROBES_GRID
		vec3 probeWorldPos = ( ( vec4( geometryPosition, 1.0 ) - viewMatrix[ 3 ] ) * viewMatrix ).xyz;
		vec3 probeWorldNormal = inverseTransformDirection( geometryNormal, viewMatrix );
		irradiance += getLightProbeGridIrradiance( probeWorldPos, probeWorldNormal );
	#endif
#endif
#if defined( RE_IndirectSpecular )
	vec3 radiance = vec3( 0.0 );
	vec3 clearcoatRadiance = vec3( 0.0 );
#endif`,LT=`#if defined( RE_IndirectDiffuse )
	#ifdef USE_LIGHTMAP
		vec4 lightMapTexel = texture2D( lightMap, vLightMapUv );
		vec3 lightMapIrradiance = lightMapTexel.rgb * lightMapIntensity;
		irradiance += lightMapIrradiance;
	#endif
	#if defined( USE_ENVMAP ) && defined( ENVMAP_TYPE_CUBE_UV )
		#if defined( STANDARD ) || defined( LAMBERT ) || defined( PHONG )
			iblIrradiance += getIBLIrradiance( geometryNormal );
		#endif
	#endif
#endif
#if defined( USE_ENVMAP ) && defined( RE_IndirectSpecular )
	#ifdef USE_ANISOTROPY
		radiance += getIBLAnisotropyRadiance( geometryViewDir, geometryNormal, material.roughness, material.anisotropyB, material.anisotropy );
	#else
		radiance += getIBLRadiance( geometryViewDir, geometryNormal, material.roughness );
	#endif
	#ifdef USE_CLEARCOAT
		clearcoatRadiance += getIBLRadiance( geometryViewDir, geometryClearcoatNormal, material.clearcoatRoughness );
	#endif
#endif`,OT=`#if defined( RE_IndirectDiffuse )
	#if defined( LAMBERT ) || defined( PHONG )
		irradiance += iblIrradiance;
	#endif
	RE_IndirectDiffuse( irradiance, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
#endif
#if defined( RE_IndirectSpecular )
	RE_IndirectSpecular( radiance, iblIrradiance, clearcoatRadiance, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
#endif`,PT=`#ifdef USE_LIGHT_PROBES_GRID
uniform highp sampler3D probesSH;
uniform vec3 probesMin;
uniform vec3 probesMax;
uniform vec3 probesResolution;
vec3 getLightProbeGridIrradiance( vec3 worldPos, vec3 worldNormal ) {
	vec3 res = probesResolution;
	vec3 gridRange = probesMax - probesMin;
	vec3 resMinusOne = res - 1.0;
	vec3 probeSpacing = gridRange / resMinusOne;
	vec3 samplePos = worldPos + worldNormal * probeSpacing * 0.5;
	vec3 uvw = clamp( ( samplePos - probesMin ) / gridRange, 0.0, 1.0 );
	uvw = uvw * resMinusOne / res + 0.5 / res;
	float nz          = res.z;
	float paddedSlices = nz + 2.0;
	float atlasDepth  = 7.0 * paddedSlices;
	float uvZBase     = uvw.z * nz + 1.0;
	vec4 s0 = texture( probesSH, vec3( uvw.xy, ( uvZBase                       ) / atlasDepth ) );
	vec4 s1 = texture( probesSH, vec3( uvw.xy, ( uvZBase +       paddedSlices   ) / atlasDepth ) );
	vec4 s2 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 2.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s3 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 3.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s4 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 4.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s5 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 5.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s6 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 6.0 * paddedSlices   ) / atlasDepth ) );
	vec3 c0 = s0.xyz;
	vec3 c1 = vec3( s0.w, s1.xy );
	vec3 c2 = vec3( s1.zw, s2.x );
	vec3 c3 = s2.yzw;
	vec3 c4 = s3.xyz;
	vec3 c5 = vec3( s3.w, s4.xy );
	vec3 c6 = vec3( s4.zw, s5.x );
	vec3 c7 = s5.yzw;
	vec3 c8 = s6.xyz;
	float x = worldNormal.x, y = worldNormal.y, z = worldNormal.z;
	vec3 result = c0 * 0.886227;
	result += c1 * 2.0 * 0.511664 * y;
	result += c2 * 2.0 * 0.511664 * z;
	result += c3 * 2.0 * 0.511664 * x;
	result += c4 * 2.0 * 0.429043 * x * y;
	result += c5 * 2.0 * 0.429043 * y * z;
	result += c6 * ( 0.743125 * z * z - 0.247708 );
	result += c7 * 2.0 * 0.429043 * x * z;
	result += c8 * 0.429043 * ( x * x - y * y );
	return max( result, vec3( 0.0 ) );
}
#endif`,IT=`#if defined( USE_LOGARITHMIC_DEPTH_BUFFER )
	gl_FragDepth = vIsPerspective == 0.0 ? gl_FragCoord.z : log2( vFragDepth ) * logDepthBufFC * 0.5;
#endif`,zT=`#if defined( USE_LOGARITHMIC_DEPTH_BUFFER )
	uniform float logDepthBufFC;
	varying float vFragDepth;
	varying float vIsPerspective;
#endif`,BT=`#ifdef USE_LOGARITHMIC_DEPTH_BUFFER
	varying float vFragDepth;
	varying float vIsPerspective;
#endif`,FT=`#ifdef USE_LOGARITHMIC_DEPTH_BUFFER
	vFragDepth = 1.0 + gl_Position.w;
	vIsPerspective = float( isPerspectiveMatrix( projectionMatrix ) );
#endif`,HT=`#ifdef USE_MAP
	vec4 sampledDiffuseColor = texture2D( map, vMapUv );
	#ifdef DECODE_VIDEO_TEXTURE
		sampledDiffuseColor = sRGBTransferEOTF( sampledDiffuseColor );
	#endif
	diffuseColor *= sampledDiffuseColor;
#endif`,GT=`#ifdef USE_MAP
	uniform sampler2D map;
#endif`,VT=`#if defined( USE_MAP ) || defined( USE_ALPHAMAP )
	#if defined( USE_POINTS_UV )
		vec2 uv = vUv;
	#else
		vec2 uv = ( uvTransform * vec3( gl_PointCoord.x, 1.0 - gl_PointCoord.y, 1 ) ).xy;
	#endif
#endif
#ifdef USE_MAP
	diffuseColor *= texture2D( map, uv );
#endif
#ifdef USE_ALPHAMAP
	diffuseColor.a *= texture2D( alphaMap, uv ).g;
#endif`,kT=`#if defined( USE_POINTS_UV )
	varying vec2 vUv;
#else
	#if defined( USE_MAP ) || defined( USE_ALPHAMAP )
		uniform mat3 uvTransform;
	#endif
#endif
#ifdef USE_MAP
	uniform sampler2D map;
#endif
#ifdef USE_ALPHAMAP
	uniform sampler2D alphaMap;
#endif`,jT=`float metalnessFactor = metalness;
#ifdef USE_METALNESSMAP
	vec4 texelMetalness = texture2D( metalnessMap, vMetalnessMapUv );
	metalnessFactor *= texelMetalness.b;
#endif`,XT=`#ifdef USE_METALNESSMAP
	uniform sampler2D metalnessMap;
#endif`,WT=`#ifdef USE_INSTANCING_MORPH
	float morphTargetInfluences[ MORPHTARGETS_COUNT ];
	float morphTargetBaseInfluence = texelFetch( morphTexture, ivec2( 0, gl_InstanceID ), 0 ).r;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		morphTargetInfluences[i] =  texelFetch( morphTexture, ivec2( i + 1, gl_InstanceID ), 0 ).r;
	}
#endif`,qT=`#if defined( USE_MORPHCOLORS )
	vColor *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		#if defined( USE_COLOR_ALPHA )
			if ( morphTargetInfluences[ i ] != 0.0 ) vColor += getMorph( gl_VertexID, i, 2 ) * morphTargetInfluences[ i ];
		#elif defined( USE_COLOR )
			if ( morphTargetInfluences[ i ] != 0.0 ) vColor += getMorph( gl_VertexID, i, 2 ).rgb * morphTargetInfluences[ i ];
		#endif
	}
#endif`,YT=`#ifdef USE_MORPHNORMALS
	objectNormal *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		if ( morphTargetInfluences[ i ] != 0.0 ) objectNormal += getMorph( gl_VertexID, i, 1 ).xyz * morphTargetInfluences[ i ];
	}
#endif`,ZT=`#ifdef USE_MORPHTARGETS
	#ifndef USE_INSTANCING_MORPH
		uniform float morphTargetBaseInfluence;
		uniform float morphTargetInfluences[ MORPHTARGETS_COUNT ];
	#endif
	uniform sampler2DArray morphTargetsTexture;
	uniform ivec2 morphTargetsTextureSize;
	vec4 getMorph( const in int vertexIndex, const in int morphTargetIndex, const in int offset ) {
		int texelIndex = vertexIndex * MORPHTARGETS_TEXTURE_STRIDE + offset;
		int y = texelIndex / morphTargetsTextureSize.x;
		int x = texelIndex - y * morphTargetsTextureSize.x;
		ivec3 morphUV = ivec3( x, y, morphTargetIndex );
		return texelFetch( morphTargetsTexture, morphUV, 0 );
	}
#endif`,KT=`#ifdef USE_MORPHTARGETS
	transformed *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		if ( morphTargetInfluences[ i ] != 0.0 ) transformed += getMorph( gl_VertexID, i, 0 ).xyz * morphTargetInfluences[ i ];
	}
#endif`,QT=`float faceDirection = gl_FrontFacing ? 1.0 : - 1.0;
#ifdef FLAT_SHADED
	vec3 fdx = dFdx( vViewPosition );
	vec3 fdy = dFdy( vViewPosition );
	vec3 normal = normalize( cross( fdx, fdy ) );
#else
	vec3 normal = normalize( vNormal );
	#ifdef DOUBLE_SIDED
		normal *= faceDirection;
	#endif
#endif
#if defined( USE_NORMALMAP_TANGENTSPACE ) || defined( USE_CLEARCOAT_NORMALMAP ) || defined( USE_ANISOTROPY )
	#ifdef USE_TANGENT
		mat3 tbn = mat3( normalize( vTangent ), normalize( vBitangent ), normal );
	#else
		mat3 tbn = getTangentFrame( - vViewPosition, normal,
		#if defined( USE_NORMALMAP )
			vNormalMapUv
		#elif defined( USE_CLEARCOAT_NORMALMAP )
			vClearcoatNormalMapUv
		#else
			vUv
		#endif
		);
	#endif
	#if defined( DOUBLE_SIDED ) && ! defined( FLAT_SHADED )
		tbn[0] *= faceDirection;
		tbn[1] *= faceDirection;
	#endif
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	#ifdef USE_TANGENT
		mat3 tbn2 = mat3( normalize( vTangent ), normalize( vBitangent ), normal );
	#else
		mat3 tbn2 = getTangentFrame( - vViewPosition, normal, vClearcoatNormalMapUv );
	#endif
	#if defined( DOUBLE_SIDED ) && ! defined( FLAT_SHADED )
		tbn2[0] *= faceDirection;
		tbn2[1] *= faceDirection;
	#endif
#endif
vec3 nonPerturbedNormal = normal;`,JT=`#ifdef USE_NORMALMAP_OBJECTSPACE
	normal = texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0;
	#ifdef FLIP_SIDED
		normal = - normal;
	#endif
	#ifdef DOUBLE_SIDED
		normal = normal * faceDirection;
	#endif
	normal = normalize( normalMatrix * normal );
#elif defined( USE_NORMALMAP_TANGENTSPACE )
	vec3 mapN = texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0;
	#if defined( USE_PACKED_NORMALMAP )
		mapN = vec3( mapN.xy, sqrt( saturate( 1.0 - dot( mapN.xy, mapN.xy ) ) ) );
	#endif
	mapN.xy *= normalScale;
	normal = normalize( tbn * mapN );
#elif defined( USE_BUMPMAP )
	normal = perturbNormalArb( - vViewPosition, normal, dHdxy_fwd(), faceDirection );
#endif`,$T=`#ifndef FLAT_SHADED
	varying vec3 vNormal;
	#ifdef USE_TANGENT
		varying vec3 vTangent;
		varying vec3 vBitangent;
	#endif
#endif`,t1=`#ifndef FLAT_SHADED
	varying vec3 vNormal;
	#ifdef USE_TANGENT
		varying vec3 vTangent;
		varying vec3 vBitangent;
	#endif
#endif`,e1=`#ifndef FLAT_SHADED
	vNormal = normalize( transformedNormal );
	#ifdef USE_TANGENT
		vTangent = normalize( transformedTangent );
		vBitangent = normalize( cross( vNormal, vTangent ) * tangent.w );
	#endif
#endif`,n1=`#ifdef USE_NORMALMAP
	uniform sampler2D normalMap;
	uniform vec2 normalScale;
#endif
#ifdef USE_NORMALMAP_OBJECTSPACE
	uniform mat3 normalMatrix;
#endif
#if ! defined ( USE_TANGENT ) && ( defined ( USE_NORMALMAP_TANGENTSPACE ) || defined ( USE_CLEARCOAT_NORMALMAP ) || defined( USE_ANISOTROPY ) )
	mat3 getTangentFrame( vec3 eye_pos, vec3 surf_norm, vec2 uv ) {
		vec3 q0 = dFdx( eye_pos.xyz );
		vec3 q1 = dFdy( eye_pos.xyz );
		vec2 st0 = dFdx( uv.st );
		vec2 st1 = dFdy( uv.st );
		vec3 N = surf_norm;
		vec3 q1perp = cross( q1, N );
		vec3 q0perp = cross( N, q0 );
		vec3 T = q1perp * st0.x + q0perp * st1.x;
		vec3 B = q1perp * st0.y + q0perp * st1.y;
		float det = max( dot( T, T ), dot( B, B ) );
		float scale = ( det == 0.0 ) ? 0.0 : inversesqrt( det );
		return mat3( T * scale, B * scale, N );
	}
#endif`,i1=`#ifdef USE_CLEARCOAT
	vec3 clearcoatNormal = nonPerturbedNormal;
#endif`,a1=`#ifdef USE_CLEARCOAT_NORMALMAP
	vec3 clearcoatMapN = texture2D( clearcoatNormalMap, vClearcoatNormalMapUv ).xyz * 2.0 - 1.0;
	clearcoatMapN.xy *= clearcoatNormalScale;
	clearcoatNormal = normalize( tbn2 * clearcoatMapN );
#endif`,s1=`#ifdef USE_CLEARCOATMAP
	uniform sampler2D clearcoatMap;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	uniform sampler2D clearcoatNormalMap;
	uniform vec2 clearcoatNormalScale;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	uniform sampler2D clearcoatRoughnessMap;
#endif`,r1=`#ifdef USE_IRIDESCENCEMAP
	uniform sampler2D iridescenceMap;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	uniform sampler2D iridescenceThicknessMap;
#endif`,o1=`#ifdef OPAQUE
diffuseColor.a = 1.0;
#endif
#ifdef USE_TRANSMISSION
diffuseColor.a *= material.transmissionAlpha;
#endif
gl_FragColor = vec4( outgoingLight, diffuseColor.a );`,l1=`vec3 packNormalToRGB( const in vec3 normal ) {
	return normalize( normal ) * 0.5 + 0.5;
}
vec3 unpackRGBToNormal( const in vec3 rgb ) {
	return 2.0 * rgb.xyz - 1.0;
}
const float PackUpscale = 256. / 255.;const float UnpackDownscale = 255. / 256.;const float ShiftRight8 = 1. / 256.;
const float Inv255 = 1. / 255.;
const vec4 PackFactors = vec4( 1.0, 256.0, 256.0 * 256.0, 256.0 * 256.0 * 256.0 );
const vec2 UnpackFactors2 = vec2( UnpackDownscale, 1.0 / PackFactors.g );
const vec3 UnpackFactors3 = vec3( UnpackDownscale / PackFactors.rg, 1.0 / PackFactors.b );
const vec4 UnpackFactors4 = vec4( UnpackDownscale / PackFactors.rgb, 1.0 / PackFactors.a );
vec4 packDepthToRGBA( const in float v ) {
	if( v <= 0.0 )
		return vec4( 0., 0., 0., 0. );
	if( v >= 1.0 )
		return vec4( 1., 1., 1., 1. );
	float vuf;
	float af = modf( v * PackFactors.a, vuf );
	float bf = modf( vuf * ShiftRight8, vuf );
	float gf = modf( vuf * ShiftRight8, vuf );
	return vec4( vuf * Inv255, gf * PackUpscale, bf * PackUpscale, af );
}
vec3 packDepthToRGB( const in float v ) {
	if( v <= 0.0 )
		return vec3( 0., 0., 0. );
	if( v >= 1.0 )
		return vec3( 1., 1., 1. );
	float vuf;
	float bf = modf( v * PackFactors.b, vuf );
	float gf = modf( vuf * ShiftRight8, vuf );
	return vec3( vuf * Inv255, gf * PackUpscale, bf );
}
vec2 packDepthToRG( const in float v ) {
	if( v <= 0.0 )
		return vec2( 0., 0. );
	if( v >= 1.0 )
		return vec2( 1., 1. );
	float vuf;
	float gf = modf( v * 256., vuf );
	return vec2( vuf * Inv255, gf );
}
float unpackRGBAToDepth( const in vec4 v ) {
	return dot( v, UnpackFactors4 );
}
float unpackRGBToDepth( const in vec3 v ) {
	return dot( v, UnpackFactors3 );
}
float unpackRGToDepth( const in vec2 v ) {
	return v.r * UnpackFactors2.r + v.g * UnpackFactors2.g;
}
vec4 pack2HalfToRGBA( const in vec2 v ) {
	vec4 r = vec4( v.x, fract( v.x * 255.0 ), v.y, fract( v.y * 255.0 ) );
	return vec4( r.x - r.y / 255.0, r.y, r.z - r.w / 255.0, r.w );
}
vec2 unpackRGBATo2Half( const in vec4 v ) {
	return vec2( v.x + ( v.y / 255.0 ), v.z + ( v.w / 255.0 ) );
}
float viewZToOrthographicDepth( const in float viewZ, const in float near, const in float far ) {
	return ( viewZ + near ) / ( near - far );
}
float orthographicDepthToViewZ( const in float depth, const in float near, const in float far ) {
	#ifdef USE_REVERSED_DEPTH_BUFFER
	
		return depth * ( far - near ) - far;
	#else
		return depth * ( near - far ) - near;
	#endif
}
float viewZToPerspectiveDepth( const in float viewZ, const in float near, const in float far ) {
	return ( ( near + viewZ ) * far ) / ( ( far - near ) * viewZ );
}
float perspectiveDepthToViewZ( const in float depth, const in float near, const in float far ) {
	
	#ifdef USE_REVERSED_DEPTH_BUFFER
		return ( near * far ) / ( ( near - far ) * depth - near );
	#else
		return ( near * far ) / ( ( far - near ) * depth - far );
	#endif
}`,c1=`#ifdef PREMULTIPLIED_ALPHA
	gl_FragColor.rgb *= gl_FragColor.a;
#endif`,u1=`vec4 mvPosition = vec4( transformed, 1.0 );
#ifdef USE_BATCHING
	mvPosition = batchingMatrix * mvPosition;
#endif
#ifdef USE_INSTANCING
	mvPosition = instanceMatrix * mvPosition;
#endif
mvPosition = modelViewMatrix * mvPosition;
gl_Position = projectionMatrix * mvPosition;`,f1=`#ifdef DITHERING
	gl_FragColor.rgb = dithering( gl_FragColor.rgb );
#endif`,d1=`#ifdef DITHERING
	vec3 dithering( vec3 color ) {
		float grid_position = rand( gl_FragCoord.xy );
		vec3 dither_shift_RGB = vec3( 0.25 / 255.0, -0.25 / 255.0, 0.25 / 255.0 );
		dither_shift_RGB = mix( 2.0 * dither_shift_RGB, -2.0 * dither_shift_RGB, grid_position );
		return color + dither_shift_RGB;
	}
#endif`,h1=`float roughnessFactor = roughness;
#ifdef USE_ROUGHNESSMAP
	vec4 texelRoughness = texture2D( roughnessMap, vRoughnessMapUv );
	roughnessFactor *= texelRoughness.g;
#endif`,p1=`#ifdef USE_ROUGHNESSMAP
	uniform sampler2D roughnessMap;
#endif`,m1=`#if NUM_SPOT_LIGHT_COORDS > 0
	varying vec4 vSpotLightCoord[ NUM_SPOT_LIGHT_COORDS ];
#endif
#if NUM_SPOT_LIGHT_MAPS > 0
	uniform sampler2D spotLightMap[ NUM_SPOT_LIGHT_MAPS ];
#endif
#ifdef USE_SHADOWMAP
	#if NUM_DIR_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform sampler2DShadow directionalShadowMap[ NUM_DIR_LIGHT_SHADOWS ];
		#else
			uniform sampler2D directionalShadowMap[ NUM_DIR_LIGHT_SHADOWS ];
		#endif
		varying vec4 vDirectionalShadowCoord[ NUM_DIR_LIGHT_SHADOWS ];
		struct DirectionalLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform DirectionalLightShadow directionalLightShadows[ NUM_DIR_LIGHT_SHADOWS ];
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform sampler2DShadow spotShadowMap[ NUM_SPOT_LIGHT_SHADOWS ];
		#else
			uniform sampler2D spotShadowMap[ NUM_SPOT_LIGHT_SHADOWS ];
		#endif
		struct SpotLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform SpotLightShadow spotLightShadows[ NUM_SPOT_LIGHT_SHADOWS ];
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform samplerCubeShadow pointShadowMap[ NUM_POINT_LIGHT_SHADOWS ];
		#elif defined( SHADOWMAP_TYPE_BASIC )
			uniform samplerCube pointShadowMap[ NUM_POINT_LIGHT_SHADOWS ];
		#endif
		varying vec4 vPointShadowCoord[ NUM_POINT_LIGHT_SHADOWS ];
		struct PointLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
			float shadowCameraNear;
			float shadowCameraFar;
		};
		uniform PointLightShadow pointLightShadows[ NUM_POINT_LIGHT_SHADOWS ];
	#endif
	#if defined( SHADOWMAP_TYPE_PCF )
		float interleavedGradientNoise( vec2 position ) {
			return fract( 52.9829189 * fract( dot( position, vec2( 0.06711056, 0.00583715 ) ) ) );
		}
		vec2 vogelDiskSample( int sampleIndex, int samplesCount, float phi ) {
			const float goldenAngle = 2.399963229728653;
			float r = sqrt( ( float( sampleIndex ) + 0.5 ) / float( samplesCount ) );
			float theta = float( sampleIndex ) * goldenAngle + phi;
			return vec2( cos( theta ), sin( theta ) ) * r;
		}
	#endif
	#if defined( SHADOWMAP_TYPE_PCF )
		float getShadow( sampler2DShadow shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			shadowCoord.z += shadowBias;
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				vec2 texelSize = vec2( 1.0 ) / shadowMapSize;
				float radius = shadowRadius * texelSize.x;
				float phi = interleavedGradientNoise( gl_FragCoord.xy ) * PI2;
				shadow = (
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 0, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 1, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 2, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 3, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 4, 5, phi ) * radius, shadowCoord.z ) )
				) * 0.2;
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#elif defined( SHADOWMAP_TYPE_VSM )
		float getShadow( sampler2D shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				shadowCoord.z -= shadowBias;
			#else
				shadowCoord.z += shadowBias;
			#endif
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				vec2 distribution = texture2D( shadowMap, shadowCoord.xy ).rg;
				float mean = distribution.x;
				float variance = distribution.y * distribution.y;
				#ifdef USE_REVERSED_DEPTH_BUFFER
					float hard_shadow = step( mean, shadowCoord.z );
				#else
					float hard_shadow = step( shadowCoord.z, mean );
				#endif
				
				if ( hard_shadow == 1.0 ) {
					shadow = 1.0;
				} else {
					variance = max( variance, 0.0000001 );
					float d = shadowCoord.z - mean;
					float p_max = variance / ( variance + d * d );
					p_max = clamp( ( p_max - 0.3 ) / 0.65, 0.0, 1.0 );
					shadow = max( hard_shadow, p_max );
				}
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#else
		float getShadow( sampler2D shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				shadowCoord.z -= shadowBias;
			#else
				shadowCoord.z += shadowBias;
			#endif
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				float depth = texture2D( shadowMap, shadowCoord.xy ).r;
				#ifdef USE_REVERSED_DEPTH_BUFFER
					shadow = step( depth, shadowCoord.z );
				#else
					shadow = step( shadowCoord.z, depth );
				#endif
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
	#if defined( SHADOWMAP_TYPE_PCF )
	float getPointShadow( samplerCubeShadow shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord, float shadowCameraNear, float shadowCameraFar ) {
		float shadow = 1.0;
		vec3 lightToPosition = shadowCoord.xyz;
		vec3 bd3D = normalize( lightToPosition );
		vec3 absVec = abs( lightToPosition );
		float viewSpaceZ = max( max( absVec.x, absVec.y ), absVec.z );
		if ( viewSpaceZ - shadowCameraFar <= 0.0 && viewSpaceZ - shadowCameraNear >= 0.0 ) {
			#ifdef USE_REVERSED_DEPTH_BUFFER
				float dp = ( shadowCameraNear * ( shadowCameraFar - viewSpaceZ ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
				dp -= shadowBias;
			#else
				float dp = ( shadowCameraFar * ( viewSpaceZ - shadowCameraNear ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
				dp += shadowBias;
			#endif
			float texelSize = shadowRadius / shadowMapSize.x;
			vec3 absDir = abs( bd3D );
			vec3 tangent = absDir.x > absDir.z ? vec3( 0.0, 1.0, 0.0 ) : vec3( 1.0, 0.0, 0.0 );
			tangent = normalize( cross( bd3D, tangent ) );
			vec3 bitangent = cross( bd3D, tangent );
			float phi = interleavedGradientNoise( gl_FragCoord.xy ) * PI2;
			vec2 sample0 = vogelDiskSample( 0, 5, phi );
			vec2 sample1 = vogelDiskSample( 1, 5, phi );
			vec2 sample2 = vogelDiskSample( 2, 5, phi );
			vec2 sample3 = vogelDiskSample( 3, 5, phi );
			vec2 sample4 = vogelDiskSample( 4, 5, phi );
			shadow = (
				texture( shadowMap, vec4( bd3D + ( tangent * sample0.x + bitangent * sample0.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample1.x + bitangent * sample1.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample2.x + bitangent * sample2.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample3.x + bitangent * sample3.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample4.x + bitangent * sample4.y ) * texelSize, dp ) )
			) * 0.2;
		}
		return mix( 1.0, shadow, shadowIntensity );
	}
	#elif defined( SHADOWMAP_TYPE_BASIC )
	float getPointShadow( samplerCube shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord, float shadowCameraNear, float shadowCameraFar ) {
		float shadow = 1.0;
		vec3 lightToPosition = shadowCoord.xyz;
		vec3 absVec = abs( lightToPosition );
		float viewSpaceZ = max( max( absVec.x, absVec.y ), absVec.z );
		if ( viewSpaceZ - shadowCameraFar <= 0.0 && viewSpaceZ - shadowCameraNear >= 0.0 ) {
			float dp = ( shadowCameraFar * ( viewSpaceZ - shadowCameraNear ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
			dp += shadowBias;
			vec3 bd3D = normalize( lightToPosition );
			float depth = textureCube( shadowMap, bd3D ).r;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				depth = 1.0 - depth;
			#endif
			shadow = step( dp, depth );
		}
		return mix( 1.0, shadow, shadowIntensity );
	}
	#endif
	#endif
#endif`,g1=`#if NUM_SPOT_LIGHT_COORDS > 0
	uniform mat4 spotLightMatrix[ NUM_SPOT_LIGHT_COORDS ];
	varying vec4 vSpotLightCoord[ NUM_SPOT_LIGHT_COORDS ];
#endif
#ifdef USE_SHADOWMAP
	#if NUM_DIR_LIGHT_SHADOWS > 0
		uniform mat4 directionalShadowMatrix[ NUM_DIR_LIGHT_SHADOWS ];
		varying vec4 vDirectionalShadowCoord[ NUM_DIR_LIGHT_SHADOWS ];
		struct DirectionalLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform DirectionalLightShadow directionalLightShadows[ NUM_DIR_LIGHT_SHADOWS ];
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
		struct SpotLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform SpotLightShadow spotLightShadows[ NUM_SPOT_LIGHT_SHADOWS ];
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		uniform mat4 pointShadowMatrix[ NUM_POINT_LIGHT_SHADOWS ];
		varying vec4 vPointShadowCoord[ NUM_POINT_LIGHT_SHADOWS ];
		struct PointLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
			float shadowCameraNear;
			float shadowCameraFar;
		};
		uniform PointLightShadow pointLightShadows[ NUM_POINT_LIGHT_SHADOWS ];
	#endif
#endif`,_1=`#if ( defined( USE_SHADOWMAP ) && ( NUM_DIR_LIGHT_SHADOWS > 0 || NUM_POINT_LIGHT_SHADOWS > 0 ) ) || ( NUM_SPOT_LIGHT_COORDS > 0 )
	#ifdef HAS_NORMAL
		vec3 shadowWorldNormal = inverseTransformDirection( transformedNormal, viewMatrix );
	#else
		vec3 shadowWorldNormal = vec3( 0.0 );
	#endif
	vec4 shadowWorldPosition;
#endif
#if defined( USE_SHADOWMAP )
	#if NUM_DIR_LIGHT_SHADOWS > 0
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_DIR_LIGHT_SHADOWS; i ++ ) {
			shadowWorldPosition = worldPosition + vec4( shadowWorldNormal * directionalLightShadows[ i ].shadowNormalBias, 0 );
			vDirectionalShadowCoord[ i ] = directionalShadowMatrix[ i ] * shadowWorldPosition;
		}
		#pragma unroll_loop_end
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_POINT_LIGHT_SHADOWS; i ++ ) {
			shadowWorldPosition = worldPosition + vec4( shadowWorldNormal * pointLightShadows[ i ].shadowNormalBias, 0 );
			vPointShadowCoord[ i ] = pointShadowMatrix[ i ] * shadowWorldPosition;
		}
		#pragma unroll_loop_end
	#endif
#endif
#if NUM_SPOT_LIGHT_COORDS > 0
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHT_COORDS; i ++ ) {
		shadowWorldPosition = worldPosition;
		#if ( defined( USE_SHADOWMAP ) && UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
			shadowWorldPosition.xyz += shadowWorldNormal * spotLightShadows[ i ].shadowNormalBias;
		#endif
		vSpotLightCoord[ i ] = spotLightMatrix[ i ] * shadowWorldPosition;
	}
	#pragma unroll_loop_end
#endif`,v1=`float getShadowMask() {
	float shadow = 1.0;
	#ifdef USE_SHADOWMAP
	#if NUM_DIR_LIGHT_SHADOWS > 0
	DirectionalLightShadow directionalLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_DIR_LIGHT_SHADOWS; i ++ ) {
		directionalLight = directionalLightShadows[ i ];
		shadow *= receiveShadow ? getShadow( directionalShadowMap[ i ], directionalLight.shadowMapSize, directionalLight.shadowIntensity, directionalLight.shadowBias, directionalLight.shadowRadius, vDirectionalShadowCoord[ i ] ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
	SpotLightShadow spotLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHT_SHADOWS; i ++ ) {
		spotLight = spotLightShadows[ i ];
		shadow *= receiveShadow ? getShadow( spotShadowMap[ i ], spotLight.shadowMapSize, spotLight.shadowIntensity, spotLight.shadowBias, spotLight.shadowRadius, vSpotLightCoord[ i ] ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0 && ( defined( SHADOWMAP_TYPE_PCF ) || defined( SHADOWMAP_TYPE_BASIC ) )
	PointLightShadow pointLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_POINT_LIGHT_SHADOWS; i ++ ) {
		pointLight = pointLightShadows[ i ];
		shadow *= receiveShadow ? getPointShadow( pointShadowMap[ i ], pointLight.shadowMapSize, pointLight.shadowIntensity, pointLight.shadowBias, pointLight.shadowRadius, vPointShadowCoord[ i ], pointLight.shadowCameraNear, pointLight.shadowCameraFar ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#endif
	return shadow;
}`,x1=`#ifdef USE_SKINNING
	mat4 boneMatX = getBoneMatrix( skinIndex.x );
	mat4 boneMatY = getBoneMatrix( skinIndex.y );
	mat4 boneMatZ = getBoneMatrix( skinIndex.z );
	mat4 boneMatW = getBoneMatrix( skinIndex.w );
#endif`,y1=`#ifdef USE_SKINNING
	uniform mat4 bindMatrix;
	uniform mat4 bindMatrixInverse;
	uniform highp sampler2D boneTexture;
	mat4 getBoneMatrix( const in float i ) {
		int size = textureSize( boneTexture, 0 ).x;
		int j = int( i ) * 4;
		int x = j % size;
		int y = j / size;
		vec4 v1 = texelFetch( boneTexture, ivec2( x, y ), 0 );
		vec4 v2 = texelFetch( boneTexture, ivec2( x + 1, y ), 0 );
		vec4 v3 = texelFetch( boneTexture, ivec2( x + 2, y ), 0 );
		vec4 v4 = texelFetch( boneTexture, ivec2( x + 3, y ), 0 );
		return mat4( v1, v2, v3, v4 );
	}
#endif`,S1=`#ifdef USE_SKINNING
	vec4 skinVertex = bindMatrix * vec4( transformed, 1.0 );
	vec4 skinned = vec4( 0.0 );
	skinned += boneMatX * skinVertex * skinWeight.x;
	skinned += boneMatY * skinVertex * skinWeight.y;
	skinned += boneMatZ * skinVertex * skinWeight.z;
	skinned += boneMatW * skinVertex * skinWeight.w;
	transformed = ( bindMatrixInverse * skinned ).xyz;
#endif`,M1=`#ifdef USE_SKINNING
	mat4 skinMatrix = mat4( 0.0 );
	skinMatrix += skinWeight.x * boneMatX;
	skinMatrix += skinWeight.y * boneMatY;
	skinMatrix += skinWeight.z * boneMatZ;
	skinMatrix += skinWeight.w * boneMatW;
	skinMatrix = bindMatrixInverse * skinMatrix * bindMatrix;
	objectNormal = vec4( skinMatrix * vec4( objectNormal, 0.0 ) ).xyz;
	#ifdef USE_TANGENT
		objectTangent = vec4( skinMatrix * vec4( objectTangent, 0.0 ) ).xyz;
	#endif
#endif`,b1=`float specularStrength;
#ifdef USE_SPECULARMAP
	vec4 texelSpecular = texture2D( specularMap, vSpecularMapUv );
	specularStrength = texelSpecular.r;
#else
	specularStrength = 1.0;
#endif`,E1=`#ifdef USE_SPECULARMAP
	uniform sampler2D specularMap;
#endif`,T1=`#if defined( TONE_MAPPING )
	gl_FragColor.rgb = toneMapping( gl_FragColor.rgb );
#endif`,A1=`#ifndef saturate
#define saturate( a ) clamp( a, 0.0, 1.0 )
#endif
uniform float toneMappingExposure;
vec3 LinearToneMapping( vec3 color ) {
	return saturate( toneMappingExposure * color );
}
vec3 ReinhardToneMapping( vec3 color ) {
	color *= toneMappingExposure;
	return saturate( color / ( vec3( 1.0 ) + color ) );
}
vec3 CineonToneMapping( vec3 color ) {
	color *= toneMappingExposure;
	color = max( vec3( 0.0 ), color - 0.004 );
	return pow( ( color * ( 6.2 * color + 0.5 ) ) / ( color * ( 6.2 * color + 1.7 ) + 0.06 ), vec3( 2.2 ) );
}
vec3 RRTAndODTFit( vec3 v ) {
	vec3 a = v * ( v + 0.0245786 ) - 0.000090537;
	vec3 b = v * ( 0.983729 * v + 0.4329510 ) + 0.238081;
	return a / b;
}
vec3 ACESFilmicToneMapping( vec3 color ) {
	const mat3 ACESInputMat = mat3(
		vec3( 0.59719, 0.07600, 0.02840 ),		vec3( 0.35458, 0.90834, 0.13383 ),
		vec3( 0.04823, 0.01566, 0.83777 )
	);
	const mat3 ACESOutputMat = mat3(
		vec3(  1.60475, -0.10208, -0.00327 ),		vec3( -0.53108,  1.10813, -0.07276 ),
		vec3( -0.07367, -0.00605,  1.07602 )
	);
	color *= toneMappingExposure / 0.6;
	color = ACESInputMat * color;
	color = RRTAndODTFit( color );
	color = ACESOutputMat * color;
	return saturate( color );
}
const mat3 LINEAR_REC2020_TO_LINEAR_SRGB = mat3(
	vec3( 1.6605, - 0.1246, - 0.0182 ),
	vec3( - 0.5876, 1.1329, - 0.1006 ),
	vec3( - 0.0728, - 0.0083, 1.1187 )
);
const mat3 LINEAR_SRGB_TO_LINEAR_REC2020 = mat3(
	vec3( 0.6274, 0.0691, 0.0164 ),
	vec3( 0.3293, 0.9195, 0.0880 ),
	vec3( 0.0433, 0.0113, 0.8956 )
);
vec3 agxDefaultContrastApprox( vec3 x ) {
	vec3 x2 = x * x;
	vec3 x4 = x2 * x2;
	return + 15.5 * x4 * x2
		- 40.14 * x4 * x
		+ 31.96 * x4
		- 6.868 * x2 * x
		+ 0.4298 * x2
		+ 0.1191 * x
		- 0.00232;
}
vec3 AgXToneMapping( vec3 color ) {
	const mat3 AgXInsetMatrix = mat3(
		vec3( 0.856627153315983, 0.137318972929847, 0.11189821299995 ),
		vec3( 0.0951212405381588, 0.761241990602591, 0.0767994186031903 ),
		vec3( 0.0482516061458583, 0.101439036467562, 0.811302368396859 )
	);
	const mat3 AgXOutsetMatrix = mat3(
		vec3( 1.1271005818144368, - 0.1413297634984383, - 0.14132976349843826 ),
		vec3( - 0.11060664309660323, 1.157823702216272, - 0.11060664309660294 ),
		vec3( - 0.016493938717834573, - 0.016493938717834257, 1.2519364065950405 )
	);
	const float AgxMinEv = - 12.47393;	const float AgxMaxEv = 4.026069;
	color *= toneMappingExposure;
	color = LINEAR_SRGB_TO_LINEAR_REC2020 * color;
	color = AgXInsetMatrix * color;
	color = max( color, 1e-10 );	color = log2( color );
	color = ( color - AgxMinEv ) / ( AgxMaxEv - AgxMinEv );
	color = clamp( color, 0.0, 1.0 );
	color = agxDefaultContrastApprox( color );
	color = AgXOutsetMatrix * color;
	color = pow( max( vec3( 0.0 ), color ), vec3( 2.2 ) );
	color = LINEAR_REC2020_TO_LINEAR_SRGB * color;
	color = clamp( color, 0.0, 1.0 );
	return color;
}
vec3 NeutralToneMapping( vec3 color ) {
	const float StartCompression = 0.8 - 0.04;
	const float Desaturation = 0.15;
	color *= toneMappingExposure;
	float x = min( color.r, min( color.g, color.b ) );
	float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
	color -= offset;
	float peak = max( color.r, max( color.g, color.b ) );
	if ( peak < StartCompression ) return color;
	float d = 1. - StartCompression;
	float newPeak = 1. - d * d / ( peak + d - StartCompression );
	color *= newPeak / peak;
	float g = 1. - 1. / ( Desaturation * ( peak - newPeak ) + 1. );
	return mix( color, vec3( newPeak ), g );
}
vec3 CustomToneMapping( vec3 color ) { return color; }`,w1=`#ifdef USE_TRANSMISSION
	material.transmission = transmission;
	material.transmissionAlpha = 1.0;
	material.thickness = thickness;
	material.attenuationDistance = attenuationDistance;
	material.attenuationColor = attenuationColor;
	#ifdef USE_TRANSMISSIONMAP
		material.transmission *= texture2D( transmissionMap, vTransmissionMapUv ).r;
	#endif
	#ifdef USE_THICKNESSMAP
		material.thickness *= texture2D( thicknessMap, vThicknessMapUv ).g;
	#endif
	vec3 pos = vWorldPosition;
	vec3 v = normalize( cameraPosition - pos );
	vec3 n = inverseTransformDirection( normal, viewMatrix );
	vec4 transmitted = getIBLVolumeRefraction(
		n, v, material.roughness, material.diffuseContribution, material.specularColorBlended, material.specularF90,
		pos, modelMatrix, viewMatrix, projectionMatrix, material.dispersion, material.ior, material.thickness,
		material.attenuationColor, material.attenuationDistance );
	material.transmissionAlpha = mix( material.transmissionAlpha, transmitted.a, material.transmission );
	totalDiffuse = mix( totalDiffuse, transmitted.rgb, material.transmission );
#endif`,R1=`#ifdef USE_TRANSMISSION
	uniform float transmission;
	uniform float thickness;
	uniform float attenuationDistance;
	uniform vec3 attenuationColor;
	#ifdef USE_TRANSMISSIONMAP
		uniform sampler2D transmissionMap;
	#endif
	#ifdef USE_THICKNESSMAP
		uniform sampler2D thicknessMap;
	#endif
	uniform vec2 transmissionSamplerSize;
	uniform sampler2D transmissionSamplerMap;
	uniform mat4 modelMatrix;
	uniform mat4 projectionMatrix;
	varying vec3 vWorldPosition;
	float w0( float a ) {
		return ( 1.0 / 6.0 ) * ( a * ( a * ( - a + 3.0 ) - 3.0 ) + 1.0 );
	}
	float w1( float a ) {
		return ( 1.0 / 6.0 ) * ( a *  a * ( 3.0 * a - 6.0 ) + 4.0 );
	}
	float w2( float a ){
		return ( 1.0 / 6.0 ) * ( a * ( a * ( - 3.0 * a + 3.0 ) + 3.0 ) + 1.0 );
	}
	float w3( float a ) {
		return ( 1.0 / 6.0 ) * ( a * a * a );
	}
	float g0( float a ) {
		return w0( a ) + w1( a );
	}
	float g1( float a ) {
		return w2( a ) + w3( a );
	}
	float h0( float a ) {
		return - 1.0 + w1( a ) / ( w0( a ) + w1( a ) );
	}
	float h1( float a ) {
		return 1.0 + w3( a ) / ( w2( a ) + w3( a ) );
	}
	vec4 bicubic( sampler2D tex, vec2 uv, vec4 texelSize, float lod ) {
		uv = uv * texelSize.zw + 0.5;
		vec2 iuv = floor( uv );
		vec2 fuv = fract( uv );
		float g0x = g0( fuv.x );
		float g1x = g1( fuv.x );
		float h0x = h0( fuv.x );
		float h1x = h1( fuv.x );
		float h0y = h0( fuv.y );
		float h1y = h1( fuv.y );
		vec2 p0 = ( vec2( iuv.x + h0x, iuv.y + h0y ) - 0.5 ) * texelSize.xy;
		vec2 p1 = ( vec2( iuv.x + h1x, iuv.y + h0y ) - 0.5 ) * texelSize.xy;
		vec2 p2 = ( vec2( iuv.x + h0x, iuv.y + h1y ) - 0.5 ) * texelSize.xy;
		vec2 p3 = ( vec2( iuv.x + h1x, iuv.y + h1y ) - 0.5 ) * texelSize.xy;
		return g0( fuv.y ) * ( g0x * textureLod( tex, p0, lod ) + g1x * textureLod( tex, p1, lod ) ) +
			g1( fuv.y ) * ( g0x * textureLod( tex, p2, lod ) + g1x * textureLod( tex, p3, lod ) );
	}
	vec4 textureBicubic( sampler2D sampler, vec2 uv, float lod ) {
		vec2 fLodSize = vec2( textureSize( sampler, int( lod ) ) );
		vec2 cLodSize = vec2( textureSize( sampler, int( lod + 1.0 ) ) );
		vec2 fLodSizeInv = 1.0 / fLodSize;
		vec2 cLodSizeInv = 1.0 / cLodSize;
		vec4 fSample = bicubic( sampler, uv, vec4( fLodSizeInv, fLodSize ), floor( lod ) );
		vec4 cSample = bicubic( sampler, uv, vec4( cLodSizeInv, cLodSize ), ceil( lod ) );
		return mix( fSample, cSample, fract( lod ) );
	}
	vec3 getVolumeTransmissionRay( const in vec3 n, const in vec3 v, const in float thickness, const in float ior, const in mat4 modelMatrix ) {
		vec3 refractionVector = refract( - v, normalize( n ), 1.0 / ior );
		vec3 modelScale;
		modelScale.x = length( vec3( modelMatrix[ 0 ].xyz ) );
		modelScale.y = length( vec3( modelMatrix[ 1 ].xyz ) );
		modelScale.z = length( vec3( modelMatrix[ 2 ].xyz ) );
		return normalize( refractionVector ) * thickness * modelScale;
	}
	float applyIorToRoughness( const in float roughness, const in float ior ) {
		return roughness * clamp( ior * 2.0 - 2.0, 0.0, 1.0 );
	}
	vec4 getTransmissionSample( const in vec2 fragCoord, const in float roughness, const in float ior ) {
		float lod = log2( transmissionSamplerSize.x ) * applyIorToRoughness( roughness, ior );
		return textureBicubic( transmissionSamplerMap, fragCoord.xy, lod );
	}
	vec3 volumeAttenuation( const in float transmissionDistance, const in vec3 attenuationColor, const in float attenuationDistance ) {
		if ( isinf( attenuationDistance ) ) {
			return vec3( 1.0 );
		} else {
			vec3 attenuationCoefficient = -log( attenuationColor ) / attenuationDistance;
			vec3 transmittance = exp( - attenuationCoefficient * transmissionDistance );			return transmittance;
		}
	}
	vec4 getIBLVolumeRefraction( const in vec3 n, const in vec3 v, const in float roughness, const in vec3 diffuseColor,
		const in vec3 specularColor, const in float specularF90, const in vec3 position, const in mat4 modelMatrix,
		const in mat4 viewMatrix, const in mat4 projMatrix, const in float dispersion, const in float ior, const in float thickness,
		const in vec3 attenuationColor, const in float attenuationDistance ) {
		vec4 transmittedLight;
		vec3 transmittance;
		#ifdef USE_DISPERSION
			float halfSpread = ( ior - 1.0 ) * 0.025 * dispersion;
			vec3 iors = vec3( ior - halfSpread, ior, ior + halfSpread );
			for ( int i = 0; i < 3; i ++ ) {
				vec3 transmissionRay = getVolumeTransmissionRay( n, v, thickness, iors[ i ], modelMatrix );
				vec3 refractedRayExit = position + transmissionRay;
				vec4 ndcPos = projMatrix * viewMatrix * vec4( refractedRayExit, 1.0 );
				vec2 refractionCoords = ndcPos.xy / ndcPos.w;
				refractionCoords += 1.0;
				refractionCoords /= 2.0;
				vec4 transmissionSample = getTransmissionSample( refractionCoords, roughness, iors[ i ] );
				transmittedLight[ i ] = transmissionSample[ i ];
				transmittedLight.a += transmissionSample.a;
				transmittance[ i ] = diffuseColor[ i ] * volumeAttenuation( length( transmissionRay ), attenuationColor, attenuationDistance )[ i ];
			}
			transmittedLight.a /= 3.0;
		#else
			vec3 transmissionRay = getVolumeTransmissionRay( n, v, thickness, ior, modelMatrix );
			vec3 refractedRayExit = position + transmissionRay;
			vec4 ndcPos = projMatrix * viewMatrix * vec4( refractedRayExit, 1.0 );
			vec2 refractionCoords = ndcPos.xy / ndcPos.w;
			refractionCoords += 1.0;
			refractionCoords /= 2.0;
			transmittedLight = getTransmissionSample( refractionCoords, roughness, ior );
			transmittance = diffuseColor * volumeAttenuation( length( transmissionRay ), attenuationColor, attenuationDistance );
		#endif
		vec3 attenuatedColor = transmittance * transmittedLight.rgb;
		vec3 F = EnvironmentBRDF( n, v, specularColor, specularF90, roughness );
		float transmittanceFactor = ( transmittance.r + transmittance.g + transmittance.b ) / 3.0;
		return vec4( ( 1.0 - F ) * attenuatedColor, 1.0 - ( 1.0 - transmittedLight.a ) * transmittanceFactor );
	}
#endif`,C1=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	varying vec2 vUv;
#endif
#ifdef USE_MAP
	varying vec2 vMapUv;
#endif
#ifdef USE_ALPHAMAP
	varying vec2 vAlphaMapUv;
#endif
#ifdef USE_LIGHTMAP
	varying vec2 vLightMapUv;
#endif
#ifdef USE_AOMAP
	varying vec2 vAoMapUv;
#endif
#ifdef USE_BUMPMAP
	varying vec2 vBumpMapUv;
#endif
#ifdef USE_NORMALMAP
	varying vec2 vNormalMapUv;
#endif
#ifdef USE_EMISSIVEMAP
	varying vec2 vEmissiveMapUv;
#endif
#ifdef USE_METALNESSMAP
	varying vec2 vMetalnessMapUv;
#endif
#ifdef USE_ROUGHNESSMAP
	varying vec2 vRoughnessMapUv;
#endif
#ifdef USE_ANISOTROPYMAP
	varying vec2 vAnisotropyMapUv;
#endif
#ifdef USE_CLEARCOATMAP
	varying vec2 vClearcoatMapUv;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	varying vec2 vClearcoatNormalMapUv;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	varying vec2 vClearcoatRoughnessMapUv;
#endif
#ifdef USE_IRIDESCENCEMAP
	varying vec2 vIridescenceMapUv;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	varying vec2 vIridescenceThicknessMapUv;
#endif
#ifdef USE_SHEEN_COLORMAP
	varying vec2 vSheenColorMapUv;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	varying vec2 vSheenRoughnessMapUv;
#endif
#ifdef USE_SPECULARMAP
	varying vec2 vSpecularMapUv;
#endif
#ifdef USE_SPECULAR_COLORMAP
	varying vec2 vSpecularColorMapUv;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	varying vec2 vSpecularIntensityMapUv;
#endif
#ifdef USE_TRANSMISSIONMAP
	uniform mat3 transmissionMapTransform;
	varying vec2 vTransmissionMapUv;
#endif
#ifdef USE_THICKNESSMAP
	uniform mat3 thicknessMapTransform;
	varying vec2 vThicknessMapUv;
#endif`,N1=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	varying vec2 vUv;
#endif
#ifdef USE_MAP
	uniform mat3 mapTransform;
	varying vec2 vMapUv;
#endif
#ifdef USE_ALPHAMAP
	uniform mat3 alphaMapTransform;
	varying vec2 vAlphaMapUv;
#endif
#ifdef USE_LIGHTMAP
	uniform mat3 lightMapTransform;
	varying vec2 vLightMapUv;
#endif
#ifdef USE_AOMAP
	uniform mat3 aoMapTransform;
	varying vec2 vAoMapUv;
#endif
#ifdef USE_BUMPMAP
	uniform mat3 bumpMapTransform;
	varying vec2 vBumpMapUv;
#endif
#ifdef USE_NORMALMAP
	uniform mat3 normalMapTransform;
	varying vec2 vNormalMapUv;
#endif
#ifdef USE_DISPLACEMENTMAP
	uniform mat3 displacementMapTransform;
	varying vec2 vDisplacementMapUv;
#endif
#ifdef USE_EMISSIVEMAP
	uniform mat3 emissiveMapTransform;
	varying vec2 vEmissiveMapUv;
#endif
#ifdef USE_METALNESSMAP
	uniform mat3 metalnessMapTransform;
	varying vec2 vMetalnessMapUv;
#endif
#ifdef USE_ROUGHNESSMAP
	uniform mat3 roughnessMapTransform;
	varying vec2 vRoughnessMapUv;
#endif
#ifdef USE_ANISOTROPYMAP
	uniform mat3 anisotropyMapTransform;
	varying vec2 vAnisotropyMapUv;
#endif
#ifdef USE_CLEARCOATMAP
	uniform mat3 clearcoatMapTransform;
	varying vec2 vClearcoatMapUv;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	uniform mat3 clearcoatNormalMapTransform;
	varying vec2 vClearcoatNormalMapUv;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	uniform mat3 clearcoatRoughnessMapTransform;
	varying vec2 vClearcoatRoughnessMapUv;
#endif
#ifdef USE_SHEEN_COLORMAP
	uniform mat3 sheenColorMapTransform;
	varying vec2 vSheenColorMapUv;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	uniform mat3 sheenRoughnessMapTransform;
	varying vec2 vSheenRoughnessMapUv;
#endif
#ifdef USE_IRIDESCENCEMAP
	uniform mat3 iridescenceMapTransform;
	varying vec2 vIridescenceMapUv;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	uniform mat3 iridescenceThicknessMapTransform;
	varying vec2 vIridescenceThicknessMapUv;
#endif
#ifdef USE_SPECULARMAP
	uniform mat3 specularMapTransform;
	varying vec2 vSpecularMapUv;
#endif
#ifdef USE_SPECULAR_COLORMAP
	uniform mat3 specularColorMapTransform;
	varying vec2 vSpecularColorMapUv;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	uniform mat3 specularIntensityMapTransform;
	varying vec2 vSpecularIntensityMapUv;
#endif
#ifdef USE_TRANSMISSIONMAP
	uniform mat3 transmissionMapTransform;
	varying vec2 vTransmissionMapUv;
#endif
#ifdef USE_THICKNESSMAP
	uniform mat3 thicknessMapTransform;
	varying vec2 vThicknessMapUv;
#endif`,D1=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	vUv = vec3( uv, 1 ).xy;
#endif
#ifdef USE_MAP
	vMapUv = ( mapTransform * vec3( MAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ALPHAMAP
	vAlphaMapUv = ( alphaMapTransform * vec3( ALPHAMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_LIGHTMAP
	vLightMapUv = ( lightMapTransform * vec3( LIGHTMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_AOMAP
	vAoMapUv = ( aoMapTransform * vec3( AOMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_BUMPMAP
	vBumpMapUv = ( bumpMapTransform * vec3( BUMPMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_NORMALMAP
	vNormalMapUv = ( normalMapTransform * vec3( NORMALMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_DISPLACEMENTMAP
	vDisplacementMapUv = ( displacementMapTransform * vec3( DISPLACEMENTMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_EMISSIVEMAP
	vEmissiveMapUv = ( emissiveMapTransform * vec3( EMISSIVEMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_METALNESSMAP
	vMetalnessMapUv = ( metalnessMapTransform * vec3( METALNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ROUGHNESSMAP
	vRoughnessMapUv = ( roughnessMapTransform * vec3( ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ANISOTROPYMAP
	vAnisotropyMapUv = ( anisotropyMapTransform * vec3( ANISOTROPYMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOATMAP
	vClearcoatMapUv = ( clearcoatMapTransform * vec3( CLEARCOATMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	vClearcoatNormalMapUv = ( clearcoatNormalMapTransform * vec3( CLEARCOAT_NORMALMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	vClearcoatRoughnessMapUv = ( clearcoatRoughnessMapTransform * vec3( CLEARCOAT_ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_IRIDESCENCEMAP
	vIridescenceMapUv = ( iridescenceMapTransform * vec3( IRIDESCENCEMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	vIridescenceThicknessMapUv = ( iridescenceThicknessMapTransform * vec3( IRIDESCENCE_THICKNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SHEEN_COLORMAP
	vSheenColorMapUv = ( sheenColorMapTransform * vec3( SHEEN_COLORMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	vSheenRoughnessMapUv = ( sheenRoughnessMapTransform * vec3( SHEEN_ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULARMAP
	vSpecularMapUv = ( specularMapTransform * vec3( SPECULARMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULAR_COLORMAP
	vSpecularColorMapUv = ( specularColorMapTransform * vec3( SPECULAR_COLORMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	vSpecularIntensityMapUv = ( specularIntensityMapTransform * vec3( SPECULAR_INTENSITYMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_TRANSMISSIONMAP
	vTransmissionMapUv = ( transmissionMapTransform * vec3( TRANSMISSIONMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_THICKNESSMAP
	vThicknessMapUv = ( thicknessMapTransform * vec3( THICKNESSMAP_UV, 1 ) ).xy;
#endif`,U1=`#if defined( USE_ENVMAP ) || defined( DISTANCE ) || defined ( USE_SHADOWMAP ) || defined ( USE_TRANSMISSION ) || NUM_SPOT_LIGHT_COORDS > 0
	vec4 worldPosition = vec4( transformed, 1.0 );
	#ifdef USE_BATCHING
		worldPosition = batchingMatrix * worldPosition;
	#endif
	#ifdef USE_INSTANCING
		worldPosition = instanceMatrix * worldPosition;
	#endif
	worldPosition = modelMatrix * worldPosition;
#endif`;const L1=`varying vec2 vUv;
uniform mat3 uvTransform;
void main() {
	vUv = ( uvTransform * vec3( uv, 1 ) ).xy;
	gl_Position = vec4( position.xy, 1.0, 1.0 );
}`,O1=`uniform sampler2D t2D;
uniform float backgroundIntensity;
varying vec2 vUv;
void main() {
	vec4 texColor = texture2D( t2D, vUv );
	#ifdef DECODE_VIDEO_TEXTURE
		texColor = vec4( mix( pow( texColor.rgb * 0.9478672986 + vec3( 0.0521327014 ), vec3( 2.4 ) ), texColor.rgb * 0.0773993808, vec3( lessThanEqual( texColor.rgb, vec3( 0.04045 ) ) ) ), texColor.w );
	#endif
	texColor.rgb *= backgroundIntensity;
	gl_FragColor = texColor;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,P1=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
	gl_Position.z = gl_Position.w;
}`,I1=`#ifdef ENVMAP_TYPE_CUBE
	uniform samplerCube envMap;
#elif defined( ENVMAP_TYPE_CUBE_UV )
	uniform sampler2D envMap;
#endif
uniform float backgroundBlurriness;
uniform float backgroundIntensity;
uniform mat3 backgroundRotation;
varying vec3 vWorldDirection;
#include <cube_uv_reflection_fragment>
void main() {
	#ifdef ENVMAP_TYPE_CUBE
		vec4 texColor = textureCube( envMap, backgroundRotation * vWorldDirection );
	#elif defined( ENVMAP_TYPE_CUBE_UV )
		vec4 texColor = textureCubeUV( envMap, backgroundRotation * vWorldDirection, backgroundBlurriness );
	#else
		vec4 texColor = vec4( 0.0, 0.0, 0.0, 1.0 );
	#endif
	texColor.rgb *= backgroundIntensity;
	gl_FragColor = texColor;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,z1=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
	gl_Position.z = gl_Position.w;
}`,B1=`uniform samplerCube tCube;
uniform float tFlip;
uniform float opacity;
varying vec3 vWorldDirection;
void main() {
	vec4 texColor = textureCube( tCube, vec3( tFlip * vWorldDirection.x, vWorldDirection.yz ) );
	gl_FragColor = texColor;
	gl_FragColor.a *= opacity;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,F1=`#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
varying vec2 vHighPrecisionZW;
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <skinbase_vertex>
	#include <morphinstance_vertex>
	#ifdef USE_DISPLACEMENTMAP
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vHighPrecisionZW = gl_Position.zw;
}`,H1=`#if DEPTH_PACKING == 3200
	uniform float opacity;
#endif
#include <common>
#include <packing>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
varying vec2 vHighPrecisionZW;
void main() {
	vec4 diffuseColor = vec4( 1.0 );
	#include <clipping_planes_fragment>
	#if DEPTH_PACKING == 3200
		diffuseColor.a = opacity;
	#endif
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <logdepthbuf_fragment>
	#ifdef USE_REVERSED_DEPTH_BUFFER
		float fragCoordZ = vHighPrecisionZW[ 0 ] / vHighPrecisionZW[ 1 ];
	#else
		float fragCoordZ = 0.5 * vHighPrecisionZW[ 0 ] / vHighPrecisionZW[ 1 ] + 0.5;
	#endif
	#if DEPTH_PACKING == 3200
		gl_FragColor = vec4( vec3( 1.0 - fragCoordZ ), opacity );
	#elif DEPTH_PACKING == 3201
		gl_FragColor = packDepthToRGBA( fragCoordZ );
	#elif DEPTH_PACKING == 3202
		gl_FragColor = vec4( packDepthToRGB( fragCoordZ ), 1.0 );
	#elif DEPTH_PACKING == 3203
		gl_FragColor = vec4( packDepthToRG( fragCoordZ ), 0.0, 1.0 );
	#endif
}`,G1=`#define DISTANCE
varying vec3 vWorldPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <skinbase_vertex>
	#include <morphinstance_vertex>
	#ifdef USE_DISPLACEMENTMAP
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <worldpos_vertex>
	#include <clipping_planes_vertex>
	vWorldPosition = worldPosition.xyz;
}`,V1=`#define DISTANCE
uniform vec3 referencePosition;
uniform float nearDistance;
uniform float farDistance;
varying vec3 vWorldPosition;
#include <common>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <clipping_planes_pars_fragment>
void main () {
	vec4 diffuseColor = vec4( 1.0 );
	#include <clipping_planes_fragment>
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	float dist = length( vWorldPosition - referencePosition );
	dist = ( dist - nearDistance ) / ( farDistance - nearDistance );
	dist = saturate( dist );
	gl_FragColor = vec4( dist, 0.0, 0.0, 1.0 );
}`,k1=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
}`,j1=`uniform sampler2D tEquirect;
varying vec3 vWorldDirection;
#include <common>
void main() {
	vec3 direction = normalize( vWorldDirection );
	vec2 sampleUV = equirectUv( direction );
	gl_FragColor = texture2D( tEquirect, sampleUV );
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,X1=`uniform float scale;
attribute float lineDistance;
varying float vLineDistance;
#include <common>
#include <uv_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	vLineDistance = scale * lineDistance;
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
}`,W1=`uniform vec3 diffuse;
uniform float opacity;
uniform float dashSize;
uniform float totalSize;
varying float vLineDistance;
#include <common>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	if ( mod( vLineDistance, totalSize ) > dashSize ) {
		discard;
	}
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,q1=`#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#if defined ( USE_ENVMAP ) || defined ( USE_SKINNING )
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinbase_vertex>
		#include <skinnormal_vertex>
		#include <defaultnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <fog_vertex>
}`,Y1=`uniform vec3 diffuse;
uniform float opacity;
#ifndef FLAT_SHADED
	varying vec3 vNormal;
#endif
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <fog_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	#ifdef USE_LIGHTMAP
		vec4 lightMapTexel = texture2D( lightMap, vLightMapUv );
		reflectedLight.indirectDiffuse += lightMapTexel.rgb * lightMapIntensity * RECIPROCAL_PI;
	#else
		reflectedLight.indirectDiffuse += vec3( 1.0 );
	#endif
	#include <aomap_fragment>
	reflectedLight.indirectDiffuse *= diffuseColor.rgb;
	vec3 outgoingLight = reflectedLight.indirectDiffuse;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,Z1=`#define LAMBERT
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,K1=`#define LAMBERT
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_lambert_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_lambert_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + totalEmissiveRadiance;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,Q1=`#define MATCAP
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <color_pars_vertex>
#include <displacementmap_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
	vViewPosition = - mvPosition.xyz;
}`,J1=`#define MATCAP
uniform vec3 diffuse;
uniform float opacity;
uniform sampler2D matcap;
varying vec3 vViewPosition;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <normal_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	vec3 viewDir = normalize( vViewPosition );
	vec3 x = normalize( vec3( viewDir.z, 0.0, - viewDir.x ) );
	vec3 y = cross( viewDir, x );
	vec2 uv = vec2( dot( x, normal ), dot( y, normal ) ) * 0.495 + 0.5;
	#ifdef USE_MATCAP
		vec4 matcapColor = texture2D( matcap, uv );
	#else
		vec4 matcapColor = vec4( vec3( mix( 0.2, 0.8, uv.y ) ), 1.0 );
	#endif
	vec3 outgoingLight = diffuseColor.rgb * matcapColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,$1=`#define NORMAL
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	varying vec3 vViewPosition;
#endif
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	vViewPosition = - mvPosition.xyz;
#endif
}`,tA=`#define NORMAL
uniform float opacity;
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	varying vec3 vViewPosition;
#endif
#include <uv_pars_fragment>
#include <normal_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( 0.0, 0.0, 0.0, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	gl_FragColor = vec4( normalize( normal ) * 0.5 + 0.5, diffuseColor.a );
	#ifdef OPAQUE
		gl_FragColor.a = 1.0;
	#endif
}`,eA=`#define PHONG
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,nA=`#define PHONG
uniform vec3 diffuse;
uniform vec3 emissive;
uniform vec3 specular;
uniform float shininess;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_phong_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_phong_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + reflectedLight.directSpecular + reflectedLight.indirectSpecular + totalEmissiveRadiance;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,iA=`#define STANDARD
varying vec3 vViewPosition;
#ifdef USE_TRANSMISSION
	varying vec3 vWorldPosition;
#endif
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
#ifdef USE_TRANSMISSION
	vWorldPosition = worldPosition.xyz;
#endif
}`,aA=`#define STANDARD
#ifdef PHYSICAL
	#define IOR
	#define USE_SPECULAR
#endif
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float roughness;
uniform float metalness;
uniform float opacity;
#ifdef IOR
	uniform float ior;
#endif
#ifdef USE_SPECULAR
	uniform float specularIntensity;
	uniform vec3 specularColor;
	#ifdef USE_SPECULAR_COLORMAP
		uniform sampler2D specularColorMap;
	#endif
	#ifdef USE_SPECULAR_INTENSITYMAP
		uniform sampler2D specularIntensityMap;
	#endif
#endif
#ifdef USE_CLEARCOAT
	uniform float clearcoat;
	uniform float clearcoatRoughness;
#endif
#ifdef USE_DISPERSION
	uniform float dispersion;
#endif
#ifdef USE_IRIDESCENCE
	uniform float iridescence;
	uniform float iridescenceIOR;
	uniform float iridescenceThicknessMinimum;
	uniform float iridescenceThicknessMaximum;
#endif
#ifdef USE_SHEEN
	uniform vec3 sheenColor;
	uniform float sheenRoughness;
	#ifdef USE_SHEEN_COLORMAP
		uniform sampler2D sheenColorMap;
	#endif
	#ifdef USE_SHEEN_ROUGHNESSMAP
		uniform sampler2D sheenRoughnessMap;
	#endif
#endif
#ifdef USE_ANISOTROPY
	uniform vec2 anisotropyVector;
	#ifdef USE_ANISOTROPYMAP
		uniform sampler2D anisotropyMap;
	#endif
#endif
varying vec3 vViewPosition;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <iridescence_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_physical_pars_fragment>
#include <transmission_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <clearcoat_pars_fragment>
#include <iridescence_pars_fragment>
#include <roughnessmap_pars_fragment>
#include <metalnessmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <roughnessmap_fragment>
	#include <metalnessmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <clearcoat_normal_fragment_begin>
	#include <clearcoat_normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_physical_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;
	vec3 totalSpecular = reflectedLight.directSpecular + reflectedLight.indirectSpecular;
	#include <transmission_fragment>
	vec3 outgoingLight = totalDiffuse + totalSpecular + totalEmissiveRadiance;
	#ifdef USE_SHEEN
 
		outgoingLight = outgoingLight + sheenSpecularDirect + sheenSpecularIndirect;
 
 	#endif
	#ifdef USE_CLEARCOAT
		float dotNVcc = saturate( dot( geometryClearcoatNormal, geometryViewDir ) );
		vec3 Fcc = F_Schlick( material.clearcoatF0, material.clearcoatF90, dotNVcc );
		outgoingLight = outgoingLight * ( 1.0 - material.clearcoat * Fcc ) + ( clearcoatSpecularDirect + clearcoatSpecularIndirect ) * material.clearcoat;
	#endif
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,sA=`#define TOON
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,rA=`#define TOON
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <gradientmap_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_toon_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_toon_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + totalEmissiveRadiance;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,oA=`uniform float size;
uniform float scale;
#include <common>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
#ifdef USE_POINTS_UV
	varying vec2 vUv;
	uniform mat3 uvTransform;
#endif
void main() {
	#ifdef USE_POINTS_UV
		vUv = ( uvTransform * vec3( uv, 1 ) ).xy;
	#endif
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <project_vertex>
	gl_PointSize = size;
	#ifdef USE_SIZEATTENUATION
		bool isPerspective = isPerspectiveMatrix( projectionMatrix );
		if ( isPerspective ) gl_PointSize *= ( scale / - mvPosition.z );
	#endif
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <worldpos_vertex>
	#include <fog_vertex>
}`,lA=`uniform vec3 diffuse;
uniform float opacity;
#include <common>
#include <color_pars_fragment>
#include <map_particle_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_particle_fragment>
	#include <color_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,cA=`#include <common>
#include <batching_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <shadowmap_pars_vertex>
void main() {
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,uA=`uniform vec3 color;
uniform float opacity;
#include <common>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <logdepthbuf_pars_fragment>
#include <shadowmap_pars_fragment>
#include <shadowmask_pars_fragment>
void main() {
	#include <logdepthbuf_fragment>
	gl_FragColor = vec4( color, opacity * ( 1.0 - getShadowMask() ) );
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,fA=`uniform float rotation;
uniform vec2 center;
#include <common>
#include <uv_pars_vertex>
#include <fog_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	vec4 mvPosition = modelViewMatrix[ 3 ];
	vec2 scale = vec2( length( modelMatrix[ 0 ].xyz ), length( modelMatrix[ 1 ].xyz ) );
	#ifndef USE_SIZEATTENUATION
		bool isPerspective = isPerspectiveMatrix( projectionMatrix );
		if ( isPerspective ) scale *= - mvPosition.z;
	#endif
	vec2 alignedPosition = ( position.xy - ( center - vec2( 0.5 ) ) ) * scale;
	vec2 rotatedPosition;
	rotatedPosition.x = cos( rotation ) * alignedPosition.x - sin( rotation ) * alignedPosition.y;
	rotatedPosition.y = sin( rotation ) * alignedPosition.x + cos( rotation ) * alignedPosition.y;
	mvPosition.xy += rotatedPosition;
	gl_Position = projectionMatrix * mvPosition;
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
}`,dA=`uniform vec3 diffuse;
uniform float opacity;
#include <common>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
}`,xe={alphahash_fragment:LE,alphahash_pars_fragment:OE,alphamap_fragment:PE,alphamap_pars_fragment:IE,alphatest_fragment:zE,alphatest_pars_fragment:BE,aomap_fragment:FE,aomap_pars_fragment:HE,batching_pars_vertex:GE,batching_vertex:VE,begin_vertex:kE,beginnormal_vertex:jE,bsdfs:XE,iridescence_fragment:WE,bumpmap_pars_fragment:qE,clipping_planes_fragment:YE,clipping_planes_pars_fragment:ZE,clipping_planes_pars_vertex:KE,clipping_planes_vertex:QE,color_fragment:JE,color_pars_fragment:$E,color_pars_vertex:tT,color_vertex:eT,common:nT,cube_uv_reflection_fragment:iT,defaultnormal_vertex:aT,displacementmap_pars_vertex:sT,displacementmap_vertex:rT,emissivemap_fragment:oT,emissivemap_pars_fragment:lT,colorspace_fragment:cT,colorspace_pars_fragment:uT,envmap_fragment:fT,envmap_common_pars_fragment:dT,envmap_pars_fragment:hT,envmap_pars_vertex:pT,envmap_physical_pars_fragment:TT,envmap_vertex:mT,fog_vertex:gT,fog_pars_vertex:_T,fog_fragment:vT,fog_pars_fragment:xT,gradientmap_pars_fragment:yT,lightmap_pars_fragment:ST,lights_lambert_fragment:MT,lights_lambert_pars_fragment:bT,lights_pars_begin:ET,lights_toon_fragment:AT,lights_toon_pars_fragment:wT,lights_phong_fragment:RT,lights_phong_pars_fragment:CT,lights_physical_fragment:NT,lights_physical_pars_fragment:DT,lights_fragment_begin:UT,lights_fragment_maps:LT,lights_fragment_end:OT,lightprobes_pars_fragment:PT,logdepthbuf_fragment:IT,logdepthbuf_pars_fragment:zT,logdepthbuf_pars_vertex:BT,logdepthbuf_vertex:FT,map_fragment:HT,map_pars_fragment:GT,map_particle_fragment:VT,map_particle_pars_fragment:kT,metalnessmap_fragment:jT,metalnessmap_pars_fragment:XT,morphinstance_vertex:WT,morphcolor_vertex:qT,morphnormal_vertex:YT,morphtarget_pars_vertex:ZT,morphtarget_vertex:KT,normal_fragment_begin:QT,normal_fragment_maps:JT,normal_pars_fragment:$T,normal_pars_vertex:t1,normal_vertex:e1,normalmap_pars_fragment:n1,clearcoat_normal_fragment_begin:i1,clearcoat_normal_fragment_maps:a1,clearcoat_pars_fragment:s1,iridescence_pars_fragment:r1,opaque_fragment:o1,packing:l1,premultiplied_alpha_fragment:c1,project_vertex:u1,dithering_fragment:f1,dithering_pars_fragment:d1,roughnessmap_fragment:h1,roughnessmap_pars_fragment:p1,shadowmap_pars_fragment:m1,shadowmap_pars_vertex:g1,shadowmap_vertex:_1,shadowmask_pars_fragment:v1,skinbase_vertex:x1,skinning_pars_vertex:y1,skinning_vertex:S1,skinnormal_vertex:M1,specularmap_fragment:b1,specularmap_pars_fragment:E1,tonemapping_fragment:T1,tonemapping_pars_fragment:A1,transmission_fragment:w1,transmission_pars_fragment:R1,uv_pars_fragment:C1,uv_pars_vertex:N1,uv_vertex:D1,worldpos_vertex:U1,background_vert:L1,background_frag:O1,backgroundCube_vert:P1,backgroundCube_frag:I1,cube_vert:z1,cube_frag:B1,depth_vert:F1,depth_frag:H1,distance_vert:G1,distance_frag:V1,equirect_vert:k1,equirect_frag:j1,linedashed_vert:X1,linedashed_frag:W1,meshbasic_vert:q1,meshbasic_frag:Y1,meshlambert_vert:Z1,meshlambert_frag:K1,meshmatcap_vert:Q1,meshmatcap_frag:J1,meshnormal_vert:$1,meshnormal_frag:tA,meshphong_vert:eA,meshphong_frag:nA,meshphysical_vert:iA,meshphysical_frag:aA,meshtoon_vert:sA,meshtoon_frag:rA,points_vert:oA,points_frag:lA,shadow_vert:cA,shadow_frag:uA,sprite_vert:fA,sprite_frag:dA},Xt={common:{diffuse:{value:new _e(16777215)},opacity:{value:1},map:{value:null},mapTransform:{value:new pe},alphaMap:{value:null},alphaMapTransform:{value:new pe},alphaTest:{value:0}},specularmap:{specularMap:{value:null},specularMapTransform:{value:new pe}},envmap:{envMap:{value:null},envMapRotation:{value:new pe},reflectivity:{value:1},ior:{value:1.5},refractionRatio:{value:.98},dfgLUT:{value:null}},aomap:{aoMap:{value:null},aoMapIntensity:{value:1},aoMapTransform:{value:new pe}},lightmap:{lightMap:{value:null},lightMapIntensity:{value:1},lightMapTransform:{value:new pe}},bumpmap:{bumpMap:{value:null},bumpMapTransform:{value:new pe},bumpScale:{value:1}},normalmap:{normalMap:{value:null},normalMapTransform:{value:new pe},normalScale:{value:new ee(1,1)}},displacementmap:{displacementMap:{value:null},displacementMapTransform:{value:new pe},displacementScale:{value:1},displacementBias:{value:0}},emissivemap:{emissiveMap:{value:null},emissiveMapTransform:{value:new pe}},metalnessmap:{metalnessMap:{value:null},metalnessMapTransform:{value:new pe}},roughnessmap:{roughnessMap:{value:null},roughnessMapTransform:{value:new pe}},gradientmap:{gradientMap:{value:null}},fog:{fogDensity:{value:25e-5},fogNear:{value:1},fogFar:{value:2e3},fogColor:{value:new _e(16777215)}},lights:{ambientLightColor:{value:[]},lightProbe:{value:[]},directionalLights:{value:[],properties:{direction:{},color:{}}},directionalLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{}}},directionalShadowMatrix:{value:[]},spotLights:{value:[],properties:{color:{},position:{},direction:{},distance:{},coneCos:{},penumbraCos:{},decay:{}}},spotLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{}}},spotLightMap:{value:[]},spotLightMatrix:{value:[]},pointLights:{value:[],properties:{color:{},position:{},decay:{},distance:{}}},pointLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{},shadowCameraNear:{},shadowCameraFar:{}}},pointShadowMatrix:{value:[]},hemisphereLights:{value:[],properties:{direction:{},skyColor:{},groundColor:{}}},rectAreaLights:{value:[],properties:{color:{},position:{},width:{},height:{}}},ltc_1:{value:null},ltc_2:{value:null},probesSH:{value:null},probesMin:{value:new k},probesMax:{value:new k},probesResolution:{value:new k}},points:{diffuse:{value:new _e(16777215)},opacity:{value:1},size:{value:1},scale:{value:1},map:{value:null},alphaMap:{value:null},alphaMapTransform:{value:new pe},alphaTest:{value:0},uvTransform:{value:new pe}},sprite:{diffuse:{value:new _e(16777215)},opacity:{value:1},center:{value:new ee(.5,.5)},rotation:{value:0},map:{value:null},mapTransform:{value:new pe},alphaMap:{value:null},alphaMapTransform:{value:new pe},alphaTest:{value:0}}},Ki={basic:{uniforms:Wn([Xt.common,Xt.specularmap,Xt.envmap,Xt.aomap,Xt.lightmap,Xt.fog]),vertexShader:xe.meshbasic_vert,fragmentShader:xe.meshbasic_frag},lambert:{uniforms:Wn([Xt.common,Xt.specularmap,Xt.envmap,Xt.aomap,Xt.lightmap,Xt.emissivemap,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,Xt.fog,Xt.lights,{emissive:{value:new _e(0)},envMapIntensity:{value:1}}]),vertexShader:xe.meshlambert_vert,fragmentShader:xe.meshlambert_frag},phong:{uniforms:Wn([Xt.common,Xt.specularmap,Xt.envmap,Xt.aomap,Xt.lightmap,Xt.emissivemap,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,Xt.fog,Xt.lights,{emissive:{value:new _e(0)},specular:{value:new _e(1118481)},shininess:{value:30},envMapIntensity:{value:1}}]),vertexShader:xe.meshphong_vert,fragmentShader:xe.meshphong_frag},standard:{uniforms:Wn([Xt.common,Xt.envmap,Xt.aomap,Xt.lightmap,Xt.emissivemap,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,Xt.roughnessmap,Xt.metalnessmap,Xt.fog,Xt.lights,{emissive:{value:new _e(0)},roughness:{value:1},metalness:{value:0},envMapIntensity:{value:1}}]),vertexShader:xe.meshphysical_vert,fragmentShader:xe.meshphysical_frag},toon:{uniforms:Wn([Xt.common,Xt.aomap,Xt.lightmap,Xt.emissivemap,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,Xt.gradientmap,Xt.fog,Xt.lights,{emissive:{value:new _e(0)}}]),vertexShader:xe.meshtoon_vert,fragmentShader:xe.meshtoon_frag},matcap:{uniforms:Wn([Xt.common,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,Xt.fog,{matcap:{value:null}}]),vertexShader:xe.meshmatcap_vert,fragmentShader:xe.meshmatcap_frag},points:{uniforms:Wn([Xt.points,Xt.fog]),vertexShader:xe.points_vert,fragmentShader:xe.points_frag},dashed:{uniforms:Wn([Xt.common,Xt.fog,{scale:{value:1},dashSize:{value:1},totalSize:{value:2}}]),vertexShader:xe.linedashed_vert,fragmentShader:xe.linedashed_frag},depth:{uniforms:Wn([Xt.common,Xt.displacementmap]),vertexShader:xe.depth_vert,fragmentShader:xe.depth_frag},normal:{uniforms:Wn([Xt.common,Xt.bumpmap,Xt.normalmap,Xt.displacementmap,{opacity:{value:1}}]),vertexShader:xe.meshnormal_vert,fragmentShader:xe.meshnormal_frag},sprite:{uniforms:Wn([Xt.sprite,Xt.fog]),vertexShader:xe.sprite_vert,fragmentShader:xe.sprite_frag},background:{uniforms:{uvTransform:{value:new pe},t2D:{value:null},backgroundIntensity:{value:1}},vertexShader:xe.background_vert,fragmentShader:xe.background_frag},backgroundCube:{uniforms:{envMap:{value:null},backgroundBlurriness:{value:0},backgroundIntensity:{value:1},backgroundRotation:{value:new pe}},vertexShader:xe.backgroundCube_vert,fragmentShader:xe.backgroundCube_frag},cube:{uniforms:{tCube:{value:null},tFlip:{value:-1},opacity:{value:1}},vertexShader:xe.cube_vert,fragmentShader:xe.cube_frag},equirect:{uniforms:{tEquirect:{value:null}},vertexShader:xe.equirect_vert,fragmentShader:xe.equirect_frag},distance:{uniforms:Wn([Xt.common,Xt.displacementmap,{referencePosition:{value:new k},nearDistance:{value:1},farDistance:{value:1e3}}]),vertexShader:xe.distance_vert,fragmentShader:xe.distance_frag},shadow:{uniforms:Wn([Xt.lights,Xt.fog,{color:{value:new _e(0)},opacity:{value:1}}]),vertexShader:xe.shadow_vert,fragmentShader:xe.shadow_frag}};Ki.physical={uniforms:Wn([Ki.standard.uniforms,{clearcoat:{value:0},clearcoatMap:{value:null},clearcoatMapTransform:{value:new pe},clearcoatNormalMap:{value:null},clearcoatNormalMapTransform:{value:new pe},clearcoatNormalScale:{value:new ee(1,1)},clearcoatRoughness:{value:0},clearcoatRoughnessMap:{value:null},clearcoatRoughnessMapTransform:{value:new pe},dispersion:{value:0},iridescence:{value:0},iridescenceMap:{value:null},iridescenceMapTransform:{value:new pe},iridescenceIOR:{value:1.3},iridescenceThicknessMinimum:{value:100},iridescenceThicknessMaximum:{value:400},iridescenceThicknessMap:{value:null},iridescenceThicknessMapTransform:{value:new pe},sheen:{value:0},sheenColor:{value:new _e(0)},sheenColorMap:{value:null},sheenColorMapTransform:{value:new pe},sheenRoughness:{value:1},sheenRoughnessMap:{value:null},sheenRoughnessMapTransform:{value:new pe},transmission:{value:0},transmissionMap:{value:null},transmissionMapTransform:{value:new pe},transmissionSamplerSize:{value:new ee},transmissionSamplerMap:{value:null},thickness:{value:0},thicknessMap:{value:null},thicknessMapTransform:{value:new pe},attenuationDistance:{value:0},attenuationColor:{value:new _e(0)},specularColor:{value:new _e(1,1,1)},specularColorMap:{value:null},specularColorMapTransform:{value:new pe},specularIntensity:{value:1},specularIntensityMap:{value:null},specularIntensityMapTransform:{value:new pe},anisotropyVector:{value:new ee},anisotropyMap:{value:null},anisotropyMapTransform:{value:new pe}}]),vertexShader:xe.meshphysical_vert,fragmentShader:xe.meshphysical_frag};const uu={r:0,b:0,g:0},hA=new tn,Qx=new pe;Qx.set(-1,0,0,0,1,0,0,0,1);function pA(r,t,n,a,l,c){const f=new _e(0);let d=l===!0?0:1,m,h,g=null,_=0,v=null;function y(w){let D=w.isScene===!0?w.background:null;if(D&&D.isTexture){const U=w.backgroundBlurriness>0;D=t.get(D,U)}return D}function E(w){let D=!1;const U=y(w);U===null?S(f,d):U&&U.isColor&&(S(U,1),D=!0);const G=r.xr.getEnvironmentBlendMode();G==="additive"?n.buffers.color.setClear(0,0,0,1,c):G==="alpha-blend"&&n.buffers.color.setClear(0,0,0,0,c),(r.autoClear||D)&&(n.buffers.depth.setTest(!0),n.buffers.depth.setMask(!0),n.buffers.color.setMask(!0),r.clear(r.autoClearColor,r.autoClearDepth,r.autoClearStencil))}function A(w,D){const U=y(D);U&&(U.isCubeTexture||U.mapping===Lu)?(h===void 0&&(h=new Gn(new Rl(1,1,1),new na({name:"BackgroundCubeMaterial",uniforms:lo(Ki.backgroundCube.uniforms),vertexShader:Ki.backgroundCube.vertexShader,fragmentShader:Ki.backgroundCube.fragmentShader,side:ti,depthTest:!1,depthWrite:!1,fog:!1,allowOverride:!1})),h.geometry.deleteAttribute("normal"),h.geometry.deleteAttribute("uv"),h.onBeforeRender=function(G,O,B){this.matrixWorld.copyPosition(B.matrixWorld)},Object.defineProperty(h.material,"envMap",{get:function(){return this.uniforms.envMap.value}}),a.update(h)),h.material.uniforms.envMap.value=U,h.material.uniforms.backgroundBlurriness.value=D.backgroundBlurriness,h.material.uniforms.backgroundIntensity.value=D.backgroundIntensity,h.material.uniforms.backgroundRotation.value.setFromMatrix4(hA.makeRotationFromEuler(D.backgroundRotation)).transpose(),U.isCubeTexture&&U.isRenderTargetTexture===!1&&h.material.uniforms.backgroundRotation.value.premultiply(Qx),h.material.toneMapped=De.getTransfer(U.colorSpace)!==je,(g!==U||_!==U.version||v!==r.toneMapping)&&(h.material.needsUpdate=!0,g=U,_=U.version,v=r.toneMapping),h.layers.enableAll(),w.unshift(h,h.geometry,h.material,0,0,null)):U&&U.isTexture&&(m===void 0&&(m=new Gn(new Pu(2,2),new na({name:"BackgroundMaterial",uniforms:lo(Ki.background.uniforms),vertexShader:Ki.background.vertexShader,fragmentShader:Ki.background.fragmentShader,side:vs,depthTest:!1,depthWrite:!1,fog:!1,allowOverride:!1})),m.geometry.deleteAttribute("normal"),Object.defineProperty(m.material,"map",{get:function(){return this.uniforms.t2D.value}}),a.update(m)),m.material.uniforms.t2D.value=U,m.material.uniforms.backgroundIntensity.value=D.backgroundIntensity,m.material.toneMapped=De.getTransfer(U.colorSpace)!==je,U.matrixAutoUpdate===!0&&U.updateMatrix(),m.material.uniforms.uvTransform.value.copy(U.matrix),(g!==U||_!==U.version||v!==r.toneMapping)&&(m.material.needsUpdate=!0,g=U,_=U.version,v=r.toneMapping),m.layers.enableAll(),w.unshift(m,m.geometry,m.material,0,0,null))}function S(w,D){w.getRGB(uu,Xx(r)),n.buffers.color.setClear(uu.r,uu.g,uu.b,D,c)}function x(){h!==void 0&&(h.geometry.dispose(),h.material.dispose(),h=void 0),m!==void 0&&(m.geometry.dispose(),m.material.dispose(),m=void 0)}return{getClearColor:function(){return f},setClearColor:function(w,D=1){f.set(w),d=D,S(f,d)},getClearAlpha:function(){return d},setClearAlpha:function(w){d=w,S(f,d)},render:E,addToRenderList:A,dispose:x}}function mA(r,t){const n=r.getParameter(r.MAX_VERTEX_ATTRIBS),a={},l=v(null);let c=l,f=!1;function d(V,$,ht,gt,q){let P=!1;const F=_(V,gt,ht,$);c!==F&&(c=F,h(c.object)),P=y(V,gt,ht,q),P&&E(V,gt,ht,q),q!==null&&t.update(q,r.ELEMENT_ARRAY_BUFFER),(P||f)&&(f=!1,U(V,$,ht,gt),q!==null&&r.bindBuffer(r.ELEMENT_ARRAY_BUFFER,t.get(q).buffer))}function m(){return r.createVertexArray()}function h(V){return r.bindVertexArray(V)}function g(V){return r.deleteVertexArray(V)}function _(V,$,ht,gt){const q=gt.wireframe===!0;let P=a[$.id];P===void 0&&(P={},a[$.id]=P);const F=V.isInstancedMesh===!0?V.id:0;let ct=P[F];ct===void 0&&(ct={},P[F]=ct);let J=ct[ht.id];J===void 0&&(J={},ct[ht.id]=J);let xt=J[q];return xt===void 0&&(xt=v(m()),J[q]=xt),xt}function v(V){const $=[],ht=[],gt=[];for(let q=0;q<n;q++)$[q]=0,ht[q]=0,gt[q]=0;return{geometry:null,program:null,wireframe:!1,newAttributes:$,enabledAttributes:ht,attributeDivisors:gt,object:V,attributes:{},index:null}}function y(V,$,ht,gt){const q=c.attributes,P=$.attributes;let F=0;const ct=ht.getAttributes();for(const J in ct)if(ct[J].location>=0){const I=q[J];let Q=P[J];if(Q===void 0&&(J==="instanceMatrix"&&V.instanceMatrix&&(Q=V.instanceMatrix),J==="instanceColor"&&V.instanceColor&&(Q=V.instanceColor)),I===void 0||I.attribute!==Q||Q&&I.data!==Q.data)return!0;F++}return c.attributesNum!==F||c.index!==gt}function E(V,$,ht,gt){const q={},P=$.attributes;let F=0;const ct=ht.getAttributes();for(const J in ct)if(ct[J].location>=0){let I=P[J];I===void 0&&(J==="instanceMatrix"&&V.instanceMatrix&&(I=V.instanceMatrix),J==="instanceColor"&&V.instanceColor&&(I=V.instanceColor));const Q={};Q.attribute=I,I&&I.data&&(Q.data=I.data),q[J]=Q,F++}c.attributes=q,c.attributesNum=F,c.index=gt}function A(){const V=c.newAttributes;for(let $=0,ht=V.length;$<ht;$++)V[$]=0}function S(V){x(V,0)}function x(V,$){const ht=c.newAttributes,gt=c.enabledAttributes,q=c.attributeDivisors;ht[V]=1,gt[V]===0&&(r.enableVertexAttribArray(V),gt[V]=1),q[V]!==$&&(r.vertexAttribDivisor(V,$),q[V]=$)}function w(){const V=c.newAttributes,$=c.enabledAttributes;for(let ht=0,gt=$.length;ht<gt;ht++)$[ht]!==V[ht]&&(r.disableVertexAttribArray(ht),$[ht]=0)}function D(V,$,ht,gt,q,P,F){F===!0?r.vertexAttribIPointer(V,$,ht,q,P):r.vertexAttribPointer(V,$,ht,gt,q,P)}function U(V,$,ht,gt){A();const q=gt.attributes,P=ht.getAttributes(),F=$.defaultAttributeValues;for(const ct in P){const J=P[ct];if(J.location>=0){let xt=q[ct];if(xt===void 0&&(ct==="instanceMatrix"&&V.instanceMatrix&&(xt=V.instanceMatrix),ct==="instanceColor"&&V.instanceColor&&(xt=V.instanceColor)),xt!==void 0){const I=xt.normalized,Q=xt.itemSize,Mt=t.get(xt);if(Mt===void 0)continue;const Rt=Mt.buffer,wt=Mt.type,st=Mt.bytesPerElement,bt=wt===r.INT||wt===r.UNSIGNED_INT||xt.gpuType===zp;if(xt.isInterleavedBufferAttribute){const Tt=xt.data,Wt=Tt.stride,re=xt.offset;if(Tt.isInstancedInterleavedBuffer){for(let ie=0;ie<J.locationSize;ie++)x(J.location+ie,Tt.meshPerAttribute);V.isInstancedMesh!==!0&&gt._maxInstanceCount===void 0&&(gt._maxInstanceCount=Tt.meshPerAttribute*Tt.count)}else for(let ie=0;ie<J.locationSize;ie++)S(J.location+ie);r.bindBuffer(r.ARRAY_BUFFER,Rt);for(let ie=0;ie<J.locationSize;ie++)D(J.location+ie,Q/J.locationSize,wt,I,Wt*st,(re+Q/J.locationSize*ie)*st,bt)}else{if(xt.isInstancedBufferAttribute){for(let Tt=0;Tt<J.locationSize;Tt++)x(J.location+Tt,xt.meshPerAttribute);V.isInstancedMesh!==!0&&gt._maxInstanceCount===void 0&&(gt._maxInstanceCount=xt.meshPerAttribute*xt.count)}else for(let Tt=0;Tt<J.locationSize;Tt++)S(J.location+Tt);r.bindBuffer(r.ARRAY_BUFFER,Rt);for(let Tt=0;Tt<J.locationSize;Tt++)D(J.location+Tt,Q/J.locationSize,wt,I,Q*st,Q/J.locationSize*Tt*st,bt)}}else if(F!==void 0){const I=F[ct];if(I!==void 0)switch(I.length){case 2:r.vertexAttrib2fv(J.location,I);break;case 3:r.vertexAttrib3fv(J.location,I);break;case 4:r.vertexAttrib4fv(J.location,I);break;default:r.vertexAttrib1fv(J.location,I)}}}}w()}function G(){z();for(const V in a){const $=a[V];for(const ht in $){const gt=$[ht];for(const q in gt){const P=gt[q];for(const F in P)g(P[F].object),delete P[F];delete gt[q]}}delete a[V]}}function O(V){if(a[V.id]===void 0)return;const $=a[V.id];for(const ht in $){const gt=$[ht];for(const q in gt){const P=gt[q];for(const F in P)g(P[F].object),delete P[F];delete gt[q]}}delete a[V.id]}function B(V){for(const $ in a){const ht=a[$];for(const gt in ht){const q=ht[gt];if(q[V.id]===void 0)continue;const P=q[V.id];for(const F in P)g(P[F].object),delete P[F];delete q[V.id]}}}function R(V){for(const $ in a){const ht=a[$],gt=V.isInstancedMesh===!0?V.id:0,q=ht[gt];if(q!==void 0){for(const P in q){const F=q[P];for(const ct in F)g(F[ct].object),delete F[ct];delete q[P]}delete ht[gt],Object.keys(ht).length===0&&delete a[$]}}}function z(){K(),f=!0,c!==l&&(c=l,h(c.object))}function K(){l.geometry=null,l.program=null,l.wireframe=!1}return{setup:d,reset:z,resetDefaultState:K,dispose:G,releaseStatesOfGeometry:O,releaseStatesOfObject:R,releaseStatesOfProgram:B,initAttributes:A,enableAttribute:S,disableUnusedAttributes:w}}function gA(r,t,n){let a;function l(m){a=m}function c(m,h){r.drawArrays(a,m,h),n.update(h,a,1)}function f(m,h,g){g!==0&&(r.drawArraysInstanced(a,m,h,g),n.update(h,a,g))}function d(m,h,g){if(g===0)return;t.get("WEBGL_multi_draw").multiDrawArraysWEBGL(a,m,0,h,0,g);let v=0;for(let y=0;y<g;y++)v+=h[y];n.update(v,a,1)}this.setMode=l,this.render=c,this.renderInstances=f,this.renderMultiDraw=d}function _A(r,t,n,a){let l;function c(){if(l!==void 0)return l;if(t.has("EXT_texture_filter_anisotropic")===!0){const B=t.get("EXT_texture_filter_anisotropic");l=r.getParameter(B.MAX_TEXTURE_MAX_ANISOTROPY_EXT)}else l=0;return l}function f(B){return!(B!==Hi&&a.convert(B)!==r.getParameter(r.IMPLEMENTATION_COLOR_READ_FORMAT))}function d(B){const R=B===Da&&(t.has("EXT_color_buffer_half_float")||t.has("EXT_color_buffer_float"));return!(B!==pi&&a.convert(B)!==r.getParameter(r.IMPLEMENTATION_COLOR_READ_TYPE)&&B!==Qi&&!R)}function m(B){if(B==="highp"){if(r.getShaderPrecisionFormat(r.VERTEX_SHADER,r.HIGH_FLOAT).precision>0&&r.getShaderPrecisionFormat(r.FRAGMENT_SHADER,r.HIGH_FLOAT).precision>0)return"highp";B="mediump"}return B==="mediump"&&r.getShaderPrecisionFormat(r.VERTEX_SHADER,r.MEDIUM_FLOAT).precision>0&&r.getShaderPrecisionFormat(r.FRAGMENT_SHADER,r.MEDIUM_FLOAT).precision>0?"mediump":"lowp"}let h=n.precision!==void 0?n.precision:"highp";const g=m(h);g!==h&&(ce("WebGLRenderer:",h,"not supported, using",g,"instead."),h=g);const _=n.logarithmicDepthBuffer===!0,v=n.reversedDepthBuffer===!0&&t.has("EXT_clip_control");n.reversedDepthBuffer===!0&&v===!1&&ce("WebGLRenderer: Unable to use reversed depth buffer due to missing EXT_clip_control extension. Fallback to default depth buffer.");const y=r.getParameter(r.MAX_TEXTURE_IMAGE_UNITS),E=r.getParameter(r.MAX_VERTEX_TEXTURE_IMAGE_UNITS),A=r.getParameter(r.MAX_TEXTURE_SIZE),S=r.getParameter(r.MAX_CUBE_MAP_TEXTURE_SIZE),x=r.getParameter(r.MAX_VERTEX_ATTRIBS),w=r.getParameter(r.MAX_VERTEX_UNIFORM_VECTORS),D=r.getParameter(r.MAX_VARYING_VECTORS),U=r.getParameter(r.MAX_FRAGMENT_UNIFORM_VECTORS),G=r.getParameter(r.MAX_SAMPLES),O=r.getParameter(r.SAMPLES);return{isWebGL2:!0,getMaxAnisotropy:c,getMaxPrecision:m,textureFormatReadable:f,textureTypeReadable:d,precision:h,logarithmicDepthBuffer:_,reversedDepthBuffer:v,maxTextures:y,maxVertexTextures:E,maxTextureSize:A,maxCubemapSize:S,maxAttributes:x,maxVertexUniforms:w,maxVaryings:D,maxFragmentUniforms:U,maxSamples:G,samples:O}}function vA(r){const t=this;let n=null,a=0,l=!1,c=!1;const f=new js,d=new pe,m={value:null,needsUpdate:!1};this.uniform=m,this.numPlanes=0,this.numIntersection=0,this.init=function(_,v){const y=_.length!==0||v||a!==0||l;return l=v,a=_.length,y},this.beginShadows=function(){c=!0,g(null)},this.endShadows=function(){c=!1},this.setGlobalState=function(_,v){n=g(_,v,0)},this.setState=function(_,v,y){const E=_.clippingPlanes,A=_.clipIntersection,S=_.clipShadows,x=r.get(_);if(!l||E===null||E.length===0||c&&!S)c?g(null):h();else{const w=c?0:a,D=w*4;let U=x.clippingState||null;m.value=U,U=g(E,v,D,y);for(let G=0;G!==D;++G)U[G]=n[G];x.clippingState=U,this.numIntersection=A?this.numPlanes:0,this.numPlanes+=w}};function h(){m.value!==n&&(m.value=n,m.needsUpdate=a>0),t.numPlanes=a,t.numIntersection=0}function g(_,v,y,E){const A=_!==null?_.length:0;let S=null;if(A!==0){if(S=m.value,E!==!0||S===null){const x=y+A*4,w=v.matrixWorldInverse;d.getNormalMatrix(w),(S===null||S.length<x)&&(S=new Float32Array(x));for(let D=0,U=y;D!==A;++D,U+=4)f.copy(_[D]).applyMatrix4(w,d),f.normal.toArray(S,U),S[U+3]=f.constant}m.value=S,m.needsUpdate=!0}return t.numPlanes=A,t.numIntersection=0,S}}const _s=4,Uv=[.125,.215,.35,.446,.526,.582],Ys=20,xA=256,dl=new Yx,Lv=new _e;let wh=null,Rh=0,Ch=0,Nh=!1;const yA=new k;class Ov{constructor(t){this._renderer=t,this._pingPongRenderTarget=null,this._lodMax=0,this._cubeSize=0,this._sizeLods=[],this._sigmas=[],this._lodMeshes=[],this._backgroundBox=null,this._cubemapMaterial=null,this._equirectMaterial=null,this._blurMaterial=null,this._ggxMaterial=null}fromScene(t,n=0,a=.1,l=100,c={}){const{size:f=256,position:d=yA}=c;wh=this._renderer.getRenderTarget(),Rh=this._renderer.getActiveCubeFace(),Ch=this._renderer.getActiveMipmapLevel(),Nh=this._renderer.xr.enabled,this._renderer.xr.enabled=!1,this._setSize(f);const m=this._allocateTargets();return m.depthBuffer=!0,this._sceneToCubeUV(t,a,l,m,d),n>0&&this._blur(m,0,0,n),this._applyPMREM(m),this._cleanup(m),m}fromEquirectangular(t,n=null){return this._fromTexture(t,n)}fromCubemap(t,n=null){return this._fromTexture(t,n)}compileCubemapShader(){this._cubemapMaterial===null&&(this._cubemapMaterial=zv(),this._compileMaterial(this._cubemapMaterial))}compileEquirectangularShader(){this._equirectMaterial===null&&(this._equirectMaterial=Iv(),this._compileMaterial(this._equirectMaterial))}dispose(){this._dispose(),this._cubemapMaterial!==null&&this._cubemapMaterial.dispose(),this._equirectMaterial!==null&&this._equirectMaterial.dispose(),this._backgroundBox!==null&&(this._backgroundBox.geometry.dispose(),this._backgroundBox.material.dispose())}_setSize(t){this._lodMax=Math.floor(Math.log2(t)),this._cubeSize=Math.pow(2,this._lodMax)}_dispose(){this._blurMaterial!==null&&this._blurMaterial.dispose(),this._ggxMaterial!==null&&this._ggxMaterial.dispose(),this._pingPongRenderTarget!==null&&this._pingPongRenderTarget.dispose();for(let t=0;t<this._lodMeshes.length;t++)this._lodMeshes[t].geometry.dispose()}_cleanup(t){this._renderer.setRenderTarget(wh,Rh,Ch),this._renderer.xr.enabled=Nh,t.scissorTest=!1,Jr(t,0,0,t.width,t.height)}_fromTexture(t,n){t.mapping===Js||t.mapping===ro?this._setSize(t.image.length===0?16:t.image[0].width||t.image[0].image.width):this._setSize(t.image.width/4),wh=this._renderer.getRenderTarget(),Rh=this._renderer.getActiveCubeFace(),Ch=this._renderer.getActiveMipmapLevel(),Nh=this._renderer.xr.enabled,this._renderer.xr.enabled=!1;const a=n||this._allocateTargets();return this._textureToCubeUV(t,a),this._applyPMREM(a),this._cleanup(a),a}_allocateTargets(){const t=3*Math.max(this._cubeSize,112),n=4*this._cubeSize,a={magFilter:Sn,minFilter:Sn,generateMipmaps:!1,type:Da,format:Hi,colorSpace:Eu,depthBuffer:!1},l=Pv(t,n,a);if(this._pingPongRenderTarget===null||this._pingPongRenderTarget.width!==t||this._pingPongRenderTarget.height!==n){this._pingPongRenderTarget!==null&&this._dispose(),this._pingPongRenderTarget=Pv(t,n,a);const{_lodMax:c}=this;({lodMeshes:this._lodMeshes,sizeLods:this._sizeLods,sigmas:this._sigmas}=SA(c)),this._blurMaterial=bA(c,t,n),this._ggxMaterial=MA(c,t,n)}return l}_compileMaterial(t){const n=new Gn(new qn,t);this._renderer.compile(n,dl)}_sceneToCubeUV(t,n,a,l,c){const m=new hi(90,1,n,a),h=[1,-1,1,1,1,1],g=[1,1,1,-1,-1,-1],_=this._renderer,v=_.autoClear,y=_.toneMapping;_.getClearColor(Lv),_.toneMapping=$i,_.autoClear=!1,_.state.buffers.depth.getReversed()&&(_.setRenderTarget(l),_.clearDepth(),_.setRenderTarget(null)),this._backgroundBox===null&&(this._backgroundBox=new Gn(new Rl,new Ws({name:"PMREM.Background",side:ti,depthWrite:!1,depthTest:!1})));const A=this._backgroundBox,S=A.material;let x=!1;const w=t.background;w?w.isColor&&(S.color.copy(w),t.background=null,x=!0):(S.color.copy(Lv),x=!0);for(let D=0;D<6;D++){const U=D%3;U===0?(m.up.set(0,h[D],0),m.position.set(c.x,c.y,c.z),m.lookAt(c.x+g[D],c.y,c.z)):U===1?(m.up.set(0,0,h[D]),m.position.set(c.x,c.y,c.z),m.lookAt(c.x,c.y+g[D],c.z)):(m.up.set(0,h[D],0),m.position.set(c.x,c.y,c.z),m.lookAt(c.x,c.y,c.z+g[D]));const G=this._cubeSize;Jr(l,U*G,D>2?G:0,G,G),_.setRenderTarget(l),x&&_.render(A,m),_.render(t,m)}_.toneMapping=y,_.autoClear=v,t.background=w}_textureToCubeUV(t,n){const a=this._renderer,l=t.mapping===Js||t.mapping===ro;l?(this._cubemapMaterial===null&&(this._cubemapMaterial=zv()),this._cubemapMaterial.uniforms.flipEnvMap.value=t.isRenderTargetTexture===!1?-1:1):this._equirectMaterial===null&&(this._equirectMaterial=Iv());const c=l?this._cubemapMaterial:this._equirectMaterial,f=this._lodMeshes[0];f.material=c;const d=c.uniforms;d.envMap.value=t;const m=this._cubeSize;Jr(n,0,0,3*m,2*m),a.setRenderTarget(n),a.render(f,dl)}_applyPMREM(t){const n=this._renderer,a=n.autoClear;n.autoClear=!1;const l=this._lodMeshes.length;for(let c=1;c<l;c++)this._applyGGXFilter(t,c-1,c);n.autoClear=a}_applyGGXFilter(t,n,a){const l=this._renderer,c=this._pingPongRenderTarget,f=this._ggxMaterial,d=this._lodMeshes[a];d.material=f;const m=f.uniforms,h=a/(this._lodMeshes.length-1),g=n/(this._lodMeshes.length-1),_=Math.sqrt(h*h-g*g),v=0+h*1.25,y=_*v,{_lodMax:E}=this,A=this._sizeLods[a],S=3*A*(a>E-_s?a-E+_s:0),x=4*(this._cubeSize-A);m.envMap.value=t.texture,m.roughness.value=y,m.mipInt.value=E-n,Jr(c,S,x,3*A,2*A),l.setRenderTarget(c),l.render(d,dl),m.envMap.value=c.texture,m.roughness.value=0,m.mipInt.value=E-a,Jr(t,S,x,3*A,2*A),l.setRenderTarget(t),l.render(d,dl)}_blur(t,n,a,l,c){const f=this._pingPongRenderTarget;this._halfBlur(t,f,n,a,l,"latitudinal",c),this._halfBlur(f,t,a,a,l,"longitudinal",c)}_halfBlur(t,n,a,l,c,f,d){const m=this._renderer,h=this._blurMaterial;f!=="latitudinal"&&f!=="longitudinal"&&Ne("blur direction must be either latitudinal or longitudinal!");const g=3,_=this._lodMeshes[l];_.material=h;const v=h.uniforms,y=this._sizeLods[a]-1,E=isFinite(c)?Math.PI/(2*y):2*Math.PI/(2*Ys-1),A=c/E,S=isFinite(c)?1+Math.floor(g*A):Ys;S>Ys&&ce(`sigmaRadians, ${c}, is too large and will clip, as it requested ${S} samples when the maximum is set to ${Ys}`);const x=[];let w=0;for(let B=0;B<Ys;++B){const R=B/A,z=Math.exp(-R*R/2);x.push(z),B===0?w+=z:B<S&&(w+=2*z)}for(let B=0;B<x.length;B++)x[B]=x[B]/w;v.envMap.value=t.texture,v.samples.value=S,v.weights.value=x,v.latitudinal.value=f==="latitudinal",d&&(v.poleAxis.value=d);const{_lodMax:D}=this;v.dTheta.value=E,v.mipInt.value=D-a;const U=this._sizeLods[l],G=3*U*(l>D-_s?l-D+_s:0),O=4*(this._cubeSize-U);Jr(n,G,O,3*U,2*U),m.setRenderTarget(n),m.render(_,dl)}}function SA(r){const t=[],n=[],a=[];let l=r;const c=r-_s+1+Uv.length;for(let f=0;f<c;f++){const d=Math.pow(2,l);t.push(d);let m=1/d;f>r-_s?m=Uv[f-r+_s-1]:f===0&&(m=0),n.push(m);const h=1/(d-2),g=-h,_=1+h,v=[g,g,_,g,_,_,g,g,_,_,g,_],y=6,E=6,A=3,S=2,x=1,w=new Float32Array(A*E*y),D=new Float32Array(S*E*y),U=new Float32Array(x*E*y);for(let O=0;O<y;O++){const B=O%3*2/3-1,R=O>2?0:-1,z=[B,R,0,B+2/3,R,0,B+2/3,R+1,0,B,R,0,B+2/3,R+1,0,B,R+1,0];w.set(z,A*E*O),D.set(v,S*E*O);const K=[O,O,O,O,O,O];U.set(K,x*E*O)}const G=new qn;G.setAttribute("position",new Ci(w,A)),G.setAttribute("uv",new Ci(D,S)),G.setAttribute("faceIndex",new Ci(U,x)),a.push(new Gn(G,null)),l>_s&&l--}return{lodMeshes:a,sizeLods:t,sigmas:n}}function Pv(r,t,n){const a=new ta(r,t,n);return a.texture.mapping=Lu,a.texture.name="PMREM.cubeUv",a.scissorTest=!0,a}function Jr(r,t,n,a,l){r.viewport.set(t,n,a,l),r.scissor.set(t,n,a,l)}function MA(r,t,n){return new na({name:"PMREMGGXConvolution",defines:{GGX_SAMPLES:xA,CUBEUV_TEXEL_WIDTH:1/t,CUBEUV_TEXEL_HEIGHT:1/n,CUBEUV_MAX_MIP:`${r}.0`},uniforms:{envMap:{value:null},roughness:{value:0},mipInt:{value:0}},vertexShader:Iu(),fragmentShader:`

			precision highp float;
			precision highp int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;
			uniform float roughness;
			uniform float mipInt;

			#define ENVMAP_TYPE_CUBE_UV
			#include <cube_uv_reflection_fragment>

			#define PI 3.14159265359

			// Van der Corput radical inverse
			float radicalInverse_VdC(uint bits) {
				bits = (bits << 16u) | (bits >> 16u);
				bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
				bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
				bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
				bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
				return float(bits) * 2.3283064365386963e-10; // / 0x100000000
			}

			// Hammersley sequence
			vec2 hammersley(uint i, uint N) {
				return vec2(float(i) / float(N), radicalInverse_VdC(i));
			}

			// GGX VNDF importance sampling (Eric Heitz 2018)
			// "Sampling the GGX Distribution of Visible Normals"
			// https://jcgt.org/published/0007/04/01/
			vec3 importanceSampleGGX_VNDF(vec2 Xi, vec3 V, float roughness) {
				float alpha = roughness * roughness;

				// Section 4.1: Orthonormal basis
				vec3 T1 = vec3(1.0, 0.0, 0.0);
				vec3 T2 = cross(V, T1);

				// Section 4.2: Parameterization of projected area
				float r = sqrt(Xi.x);
				float phi = 2.0 * PI * Xi.y;
				float t1 = r * cos(phi);
				float t2 = r * sin(phi);
				float s = 0.5 * (1.0 + V.z);
				t2 = (1.0 - s) * sqrt(1.0 - t1 * t1) + s * t2;

				// Section 4.3: Reprojection onto hemisphere
				vec3 Nh = t1 * T1 + t2 * T2 + sqrt(max(0.0, 1.0 - t1 * t1 - t2 * t2)) * V;

				// Section 3.4: Transform back to ellipsoid configuration
				return normalize(vec3(alpha * Nh.x, alpha * Nh.y, max(0.0, Nh.z)));
			}

			void main() {
				vec3 N = normalize(vOutputDirection);
				vec3 V = N; // Assume view direction equals normal for pre-filtering

				vec3 prefilteredColor = vec3(0.0);
				float totalWeight = 0.0;

				// For very low roughness, just sample the environment directly
				if (roughness < 0.001) {
					gl_FragColor = vec4(bilinearCubeUV(envMap, N, mipInt), 1.0);
					return;
				}

				// Tangent space basis for VNDF sampling
				vec3 up = abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
				vec3 tangent = normalize(cross(up, N));
				vec3 bitangent = cross(N, tangent);

				for(uint i = 0u; i < uint(GGX_SAMPLES); i++) {
					vec2 Xi = hammersley(i, uint(GGX_SAMPLES));

					// For PMREM, V = N, so in tangent space V is always (0, 0, 1)
					vec3 H_tangent = importanceSampleGGX_VNDF(Xi, vec3(0.0, 0.0, 1.0), roughness);

					// Transform H back to world space
					vec3 H = normalize(tangent * H_tangent.x + bitangent * H_tangent.y + N * H_tangent.z);
					vec3 L = normalize(2.0 * dot(V, H) * H - V);

					float NdotL = max(dot(N, L), 0.0);

					if(NdotL > 0.0) {
						// Sample environment at fixed mip level
						// VNDF importance sampling handles the distribution filtering
						vec3 sampleColor = bilinearCubeUV(envMap, L, mipInt);

						// Weight by NdotL for the split-sum approximation
						// VNDF PDF naturally accounts for the visible microfacet distribution
						prefilteredColor += sampleColor * NdotL;
						totalWeight += NdotL;
					}
				}

				if (totalWeight > 0.0) {
					prefilteredColor = prefilteredColor / totalWeight;
				}

				gl_FragColor = vec4(prefilteredColor, 1.0);
			}
		`,blending:Ra,depthTest:!1,depthWrite:!1})}function bA(r,t,n){const a=new Float32Array(Ys),l=new k(0,1,0);return new na({name:"SphericalGaussianBlur",defines:{n:Ys,CUBEUV_TEXEL_WIDTH:1/t,CUBEUV_TEXEL_HEIGHT:1/n,CUBEUV_MAX_MIP:`${r}.0`},uniforms:{envMap:{value:null},samples:{value:1},weights:{value:a},latitudinal:{value:!1},dTheta:{value:0},mipInt:{value:0},poleAxis:{value:l}},vertexShader:Iu(),fragmentShader:`

			precision mediump float;
			precision mediump int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;
			uniform int samples;
			uniform float weights[ n ];
			uniform bool latitudinal;
			uniform float dTheta;
			uniform float mipInt;
			uniform vec3 poleAxis;

			#define ENVMAP_TYPE_CUBE_UV
			#include <cube_uv_reflection_fragment>

			vec3 getSample( float theta, vec3 axis ) {

				float cosTheta = cos( theta );
				// Rodrigues' axis-angle rotation
				vec3 sampleDirection = vOutputDirection * cosTheta
					+ cross( axis, vOutputDirection ) * sin( theta )
					+ axis * dot( axis, vOutputDirection ) * ( 1.0 - cosTheta );

				return bilinearCubeUV( envMap, sampleDirection, mipInt );

			}

			void main() {

				vec3 axis = latitudinal ? poleAxis : cross( poleAxis, vOutputDirection );

				if ( all( equal( axis, vec3( 0.0 ) ) ) ) {

					axis = vec3( vOutputDirection.z, 0.0, - vOutputDirection.x );

				}

				axis = normalize( axis );

				gl_FragColor = vec4( 0.0, 0.0, 0.0, 1.0 );
				gl_FragColor.rgb += weights[ 0 ] * getSample( 0.0, axis );

				for ( int i = 1; i < n; i++ ) {

					if ( i >= samples ) {

						break;

					}

					float theta = dTheta * float( i );
					gl_FragColor.rgb += weights[ i ] * getSample( -1.0 * theta, axis );
					gl_FragColor.rgb += weights[ i ] * getSample( theta, axis );

				}

			}
		`,blending:Ra,depthTest:!1,depthWrite:!1})}function Iv(){return new na({name:"EquirectangularToCubeUV",uniforms:{envMap:{value:null}},vertexShader:Iu(),fragmentShader:`

			precision mediump float;
			precision mediump int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;

			#include <common>

			void main() {

				vec3 outputDirection = normalize( vOutputDirection );
				vec2 uv = equirectUv( outputDirection );

				gl_FragColor = vec4( texture2D ( envMap, uv ).rgb, 1.0 );

			}
		`,blending:Ra,depthTest:!1,depthWrite:!1})}function zv(){return new na({name:"CubemapToCubeUV",uniforms:{envMap:{value:null},flipEnvMap:{value:-1}},vertexShader:Iu(),fragmentShader:`

			precision mediump float;
			precision mediump int;

			uniform float flipEnvMap;

			varying vec3 vOutputDirection;

			uniform samplerCube envMap;

			void main() {

				gl_FragColor = textureCube( envMap, vec3( flipEnvMap * vOutputDirection.x, vOutputDirection.yz ) );

			}
		`,blending:Ra,depthTest:!1,depthWrite:!1})}function Iu(){return`

		precision mediump float;
		precision mediump int;

		attribute float faceIndex;

		varying vec3 vOutputDirection;

		// RH coordinate system; PMREM face-indexing convention
		vec3 getDirection( vec2 uv, float face ) {

			uv = 2.0 * uv - 1.0;

			vec3 direction = vec3( uv, 1.0 );

			if ( face == 0.0 ) {

				direction = direction.zyx; // ( 1, v, u ) pos x

			} else if ( face == 1.0 ) {

				direction = direction.xzy;
				direction.xz *= -1.0; // ( -u, 1, -v ) pos y

			} else if ( face == 2.0 ) {

				direction.x *= -1.0; // ( -u, v, 1 ) pos z

			} else if ( face == 3.0 ) {

				direction = direction.zyx;
				direction.xz *= -1.0; // ( -1, v, -u ) neg x

			} else if ( face == 4.0 ) {

				direction = direction.xzy;
				direction.xy *= -1.0; // ( -u, -1, v ) neg y

			} else if ( face == 5.0 ) {

				direction.z *= -1.0; // ( u, v, -1 ) neg z

			}

			return direction;

		}

		void main() {

			vOutputDirection = getDirection( uv, faceIndex );
			gl_Position = vec4( position, 1.0 );

		}
	`}class Jx extends ta{constructor(t=1,n={}){super(t,t,n),this.isWebGLCubeRenderTarget=!0;const a={width:t,height:t,depth:1},l=[a,a,a,a,a,a];this.texture=new Hx(l),this._setTextureOptions(n),this.texture.isRenderTargetTexture=!0}fromEquirectangularTexture(t,n){this.texture.type=n.type,this.texture.colorSpace=n.colorSpace,this.texture.generateMipmaps=n.generateMipmaps,this.texture.minFilter=n.minFilter,this.texture.magFilter=n.magFilter;const a={uniforms:{tEquirect:{value:null}},vertexShader:`

				varying vec3 vWorldDirection;

				vec3 transformDirection( in vec3 dir, in mat4 matrix ) {

					return normalize( ( matrix * vec4( dir, 0.0 ) ).xyz );

				}

				void main() {

					vWorldDirection = transformDirection( position, modelMatrix );

					#include <begin_vertex>
					#include <project_vertex>

				}
			`,fragmentShader:`

				uniform sampler2D tEquirect;

				varying vec3 vWorldDirection;

				#include <common>

				void main() {

					vec3 direction = normalize( vWorldDirection );

					vec2 sampleUV = equirectUv( direction );

					gl_FragColor = texture2D( tEquirect, sampleUV );

				}
			`},l=new Rl(5,5,5),c=new na({name:"CubemapFromEquirect",uniforms:lo(a.uniforms),vertexShader:a.vertexShader,fragmentShader:a.fragmentShader,side:ti,blending:Ra});c.uniforms.tEquirect.value=n;const f=new Gn(l,c),d=n.minFilter;return n.minFilter===Zs&&(n.minFilter=Sn),new RE(1,10,this).update(t,f),n.minFilter=d,f.geometry.dispose(),f.material.dispose(),this}clear(t,n=!0,a=!0,l=!0){const c=t.getRenderTarget();for(let f=0;f<6;f++)t.setRenderTarget(this,f),t.clear(n,a,l);t.setRenderTarget(c)}}function EA(r){let t=new WeakMap,n=new WeakMap,a=null;function l(v,y=!1){return v==null?null:y?f(v):c(v)}function c(v){if(v&&v.isTexture){const y=v.mapping;if(y===Qd||y===Jd)if(t.has(v)){const E=t.get(v).texture;return d(E,v.mapping)}else{const E=v.image;if(E&&E.height>0){const A=new Jx(E.height);return A.fromEquirectangularTexture(r,v),t.set(v,A),v.addEventListener("dispose",h),d(A.texture,v.mapping)}else return null}}return v}function f(v){if(v&&v.isTexture){const y=v.mapping,E=y===Qd||y===Jd,A=y===Js||y===ro;if(E||A){let S=n.get(v);const x=S!==void 0?S.texture.pmremVersion:0;if(v.isRenderTargetTexture&&v.pmremVersion!==x)return a===null&&(a=new Ov(r)),S=E?a.fromEquirectangular(v,S):a.fromCubemap(v,S),S.texture.pmremVersion=v.pmremVersion,n.set(v,S),S.texture;if(S!==void 0)return S.texture;{const w=v.image;return E&&w&&w.height>0||A&&w&&m(w)?(a===null&&(a=new Ov(r)),S=E?a.fromEquirectangular(v):a.fromCubemap(v),S.texture.pmremVersion=v.pmremVersion,n.set(v,S),v.addEventListener("dispose",g),S.texture):null}}}return v}function d(v,y){return y===Qd?v.mapping=Js:y===Jd&&(v.mapping=ro),v}function m(v){let y=0;const E=6;for(let A=0;A<E;A++)v[A]!==void 0&&y++;return y===E}function h(v){const y=v.target;y.removeEventListener("dispose",h);const E=t.get(y);E!==void 0&&(t.delete(y),E.dispose())}function g(v){const y=v.target;y.removeEventListener("dispose",g);const E=n.get(y);E!==void 0&&(n.delete(y),E.dispose())}function _(){t=new WeakMap,n=new WeakMap,a!==null&&(a.dispose(),a=null)}return{get:l,dispose:_}}function TA(r){const t={};function n(a){if(t[a]!==void 0)return t[a];const l=r.getExtension(a);return t[a]=l,l}return{has:function(a){return n(a)!==null},init:function(){n("EXT_color_buffer_float"),n("WEBGL_clip_cull_distance"),n("OES_texture_float_linear"),n("EXT_color_buffer_half_float"),n("WEBGL_multisampled_render_to_texture"),n("WEBGL_render_shared_exponent")},get:function(a){const l=n(a);return l===null&&Tp("WebGLRenderer: "+a+" extension not supported."),l}}}function AA(r,t,n,a){const l={},c=new WeakMap;function f(_){const v=_.target;v.index!==null&&t.remove(v.index);for(const E in v.attributes)t.remove(v.attributes[E]);v.removeEventListener("dispose",f),delete l[v.id];const y=c.get(v);y&&(t.remove(y),c.delete(v)),a.releaseStatesOfGeometry(v),v.isInstancedBufferGeometry===!0&&delete v._maxInstanceCount,n.memory.geometries--}function d(_,v){return l[v.id]===!0||(v.addEventListener("dispose",f),l[v.id]=!0,n.memory.geometries++),v}function m(_){const v=_.attributes;for(const y in v)t.update(v[y],r.ARRAY_BUFFER)}function h(_){const v=[],y=_.index,E=_.attributes.position;let A=0;if(E===void 0)return;if(y!==null){const w=y.array;A=y.version;for(let D=0,U=w.length;D<U;D+=3){const G=w[D+0],O=w[D+1],B=w[D+2];v.push(G,O,O,B,B,G)}}else{const w=E.array;A=E.version;for(let D=0,U=w.length/3-1;D<U;D+=3){const G=D+0,O=D+1,B=D+2;v.push(G,O,O,B,B,G)}}const S=new(E.count>=65535?zx:Ix)(v,1);S.version=A;const x=c.get(_);x&&t.remove(x),c.set(_,S)}function g(_){const v=c.get(_);if(v){const y=_.index;y!==null&&v.version<y.version&&h(_)}else h(_);return c.get(_)}return{get:d,update:m,getWireframeAttribute:g}}function wA(r,t,n){let a;function l(_){a=_}let c,f;function d(_){c=_.type,f=_.bytesPerElement}function m(_,v){r.drawElements(a,v,c,_*f),n.update(v,a,1)}function h(_,v,y){y!==0&&(r.drawElementsInstanced(a,v,c,_*f,y),n.update(v,a,y))}function g(_,v,y){if(y===0)return;t.get("WEBGL_multi_draw").multiDrawElementsWEBGL(a,v,0,c,_,0,y);let A=0;for(let S=0;S<y;S++)A+=v[S];n.update(A,a,1)}this.setMode=l,this.setIndex=d,this.render=m,this.renderInstances=h,this.renderMultiDraw=g}function RA(r){const t={geometries:0,textures:0},n={frame:0,calls:0,triangles:0,points:0,lines:0};function a(c,f,d){switch(n.calls++,f){case r.TRIANGLES:n.triangles+=d*(c/3);break;case r.LINES:n.lines+=d*(c/2);break;case r.LINE_STRIP:n.lines+=d*(c-1);break;case r.LINE_LOOP:n.lines+=d*c;break;case r.POINTS:n.points+=d*c;break;default:Ne("WebGLInfo: Unknown draw mode:",f);break}}function l(){n.calls=0,n.triangles=0,n.points=0,n.lines=0}return{memory:t,render:n,programs:null,autoReset:!0,reset:l,update:a}}function CA(r,t,n){const a=new WeakMap,l=new fn;function c(f,d,m){const h=f.morphTargetInfluences,g=d.morphAttributes.position||d.morphAttributes.normal||d.morphAttributes.color,_=g!==void 0?g.length:0;let v=a.get(d);if(v===void 0||v.count!==_){let K=function(){R.dispose(),a.delete(d),d.removeEventListener("dispose",K)};var y=K;v!==void 0&&v.texture.dispose();const E=d.morphAttributes.position!==void 0,A=d.morphAttributes.normal!==void 0,S=d.morphAttributes.color!==void 0,x=d.morphAttributes.position||[],w=d.morphAttributes.normal||[],D=d.morphAttributes.color||[];let U=0;E===!0&&(U=1),A===!0&&(U=2),S===!0&&(U=3);let G=d.attributes.position.count*U,O=1;G>t.maxTextureSize&&(O=Math.ceil(G/t.maxTextureSize),G=t.maxTextureSize);const B=new Float32Array(G*O*4*_),R=new Ox(B,G,O,_);R.type=Qi,R.needsUpdate=!0;const z=U*4;for(let V=0;V<_;V++){const $=x[V],ht=w[V],gt=D[V],q=G*O*4*V;for(let P=0;P<$.count;P++){const F=P*z;E===!0&&(l.fromBufferAttribute($,P),B[q+F+0]=l.x,B[q+F+1]=l.y,B[q+F+2]=l.z,B[q+F+3]=0),A===!0&&(l.fromBufferAttribute(ht,P),B[q+F+4]=l.x,B[q+F+5]=l.y,B[q+F+6]=l.z,B[q+F+7]=0),S===!0&&(l.fromBufferAttribute(gt,P),B[q+F+8]=l.x,B[q+F+9]=l.y,B[q+F+10]=l.z,B[q+F+11]=gt.itemSize===4?l.w:1)}}v={count:_,texture:R,size:new ee(G,O)},a.set(d,v),d.addEventListener("dispose",K)}if(f.isInstancedMesh===!0&&f.morphTexture!==null)m.getUniforms().setValue(r,"morphTexture",f.morphTexture,n);else{let E=0;for(let S=0;S<h.length;S++)E+=h[S];const A=d.morphTargetsRelative?1:1-E;m.getUniforms().setValue(r,"morphTargetBaseInfluence",A),m.getUniforms().setValue(r,"morphTargetInfluences",h)}m.getUniforms().setValue(r,"morphTargetsTexture",v.texture,n),m.getUniforms().setValue(r,"morphTargetsTextureSize",v.size)}return{update:c}}function NA(r,t,n,a,l){let c=new WeakMap;function f(h){const g=l.render.frame,_=h.geometry,v=t.get(h,_);if(c.get(v)!==g&&(t.update(v),c.set(v,g)),h.isInstancedMesh&&(h.hasEventListener("dispose",m)===!1&&h.addEventListener("dispose",m),c.get(h)!==g&&(n.update(h.instanceMatrix,r.ARRAY_BUFFER),h.instanceColor!==null&&n.update(h.instanceColor,r.ARRAY_BUFFER),c.set(h,g))),h.isSkinnedMesh){const y=h.skeleton;c.get(y)!==g&&(y.update(),c.set(y,g))}return v}function d(){c=new WeakMap}function m(h){const g=h.target;g.removeEventListener("dispose",m),a.releaseStatesOfObject(g),n.remove(g.instanceMatrix),g.instanceColor!==null&&n.remove(g.instanceColor)}return{update:f,dispose:d}}const DA={[vx]:"LINEAR_TONE_MAPPING",[xx]:"REINHARD_TONE_MAPPING",[yx]:"CINEON_TONE_MAPPING",[Sx]:"ACES_FILMIC_TONE_MAPPING",[bx]:"AGX_TONE_MAPPING",[Ex]:"NEUTRAL_TONE_MAPPING",[Mx]:"CUSTOM_TONE_MAPPING"};function UA(r,t,n,a,l){const c=new ta(t,n,{type:r,depthBuffer:a,stencilBuffer:l,depthTexture:a?new oo(t,n):void 0}),f=new ta(t,n,{type:Da,depthBuffer:!1,stencilBuffer:!1}),d=new qn;d.setAttribute("position",new Rn([-1,3,0,-1,-1,0,3,-1,0],3)),d.setAttribute("uv",new Rn([0,2,0,0,2,0],2));const m=new SE({uniforms:{tDiffuse:{value:null}},vertexShader:`
			precision highp float;

			uniform mat4 modelViewMatrix;
			uniform mat4 projectionMatrix;

			attribute vec3 position;
			attribute vec2 uv;

			varying vec2 vUv;

			void main() {
				vUv = uv;
				gl_Position = projectionMatrix * modelViewMatrix * vec4( position, 1.0 );
			}`,fragmentShader:`
			precision highp float;

			uniform sampler2D tDiffuse;

			varying vec2 vUv;

			#include <tonemapping_pars_fragment>
			#include <colorspace_pars_fragment>

			void main() {
				gl_FragColor = texture2D( tDiffuse, vUv );

				#ifdef LINEAR_TONE_MAPPING
					gl_FragColor.rgb = LinearToneMapping( gl_FragColor.rgb );
				#elif defined( REINHARD_TONE_MAPPING )
					gl_FragColor.rgb = ReinhardToneMapping( gl_FragColor.rgb );
				#elif defined( CINEON_TONE_MAPPING )
					gl_FragColor.rgb = CineonToneMapping( gl_FragColor.rgb );
				#elif defined( ACES_FILMIC_TONE_MAPPING )
					gl_FragColor.rgb = ACESFilmicToneMapping( gl_FragColor.rgb );
				#elif defined( AGX_TONE_MAPPING )
					gl_FragColor.rgb = AgXToneMapping( gl_FragColor.rgb );
				#elif defined( NEUTRAL_TONE_MAPPING )
					gl_FragColor.rgb = NeutralToneMapping( gl_FragColor.rgb );
				#elif defined( CUSTOM_TONE_MAPPING )
					gl_FragColor.rgb = CustomToneMapping( gl_FragColor.rgb );
				#endif

				#ifdef SRGB_TRANSFER
					gl_FragColor = sRGBTransferOETF( gl_FragColor );
				#endif
			}`,depthTest:!1,depthWrite:!1}),h=new Gn(d,m),g=new Yx(-1,1,1,-1,0,1);let _=null,v=null,y=!1,E,A=null,S=[],x=!1;this.setSize=function(w,D){c.setSize(w,D),f.setSize(w,D);for(let U=0;U<S.length;U++){const G=S[U];G.setSize&&G.setSize(w,D)}},this.setEffects=function(w){S=w,x=S.length>0&&S[0].isRenderPass===!0;const D=c.width,U=c.height;for(let G=0;G<S.length;G++){const O=S[G];O.setSize&&O.setSize(D,U)}},this.begin=function(w,D){if(y||w.toneMapping===$i&&S.length===0)return!1;if(A=D,D!==null){const U=D.width,G=D.height;(c.width!==U||c.height!==G)&&this.setSize(U,G)}return x===!1&&w.setRenderTarget(c),E=w.toneMapping,w.toneMapping=$i,!0},this.hasRenderPass=function(){return x},this.end=function(w,D){w.toneMapping=E,y=!0;let U=c,G=f;for(let O=0;O<S.length;O++){const B=S[O];if(B.enabled!==!1&&(B.render(w,G,U,D),B.needsSwap!==!1)){const R=U;U=G,G=R}}if(_!==w.outputColorSpace||v!==w.toneMapping){_=w.outputColorSpace,v=w.toneMapping,m.defines={},De.getTransfer(_)===je&&(m.defines.SRGB_TRANSFER="");const O=DA[v];O&&(m.defines[O]=""),m.needsUpdate=!0}m.uniforms.tDiffuse.value=U.texture,w.setRenderTarget(A),w.render(h,g),A=null,y=!1},this.isCompositing=function(){return y},this.dispose=function(){c.depthTexture&&c.depthTexture.dispose(),c.dispose(),f.dispose(),d.dispose(),m.dispose()}}const $x=new Vn,Rp=new oo(1,1),ty=new Ox,ey=new Pb,ny=new Hx,Bv=[],Fv=[],Hv=new Float32Array(16),Gv=new Float32Array(9),Vv=new Float32Array(4);function co(r,t,n){const a=r[0];if(a<=0||a>0)return r;const l=t*n;let c=Bv[l];if(c===void 0&&(c=new Float32Array(l),Bv[l]=c),t!==0){a.toArray(c,0);for(let f=1,d=0;f!==t;++f)d+=n,r[f].toArray(c,d)}return c}function En(r,t){if(r.length!==t.length)return!1;for(let n=0,a=r.length;n<a;n++)if(r[n]!==t[n])return!1;return!0}function Tn(r,t){for(let n=0,a=t.length;n<a;n++)r[n]=t[n]}function zu(r,t){let n=Fv[t];n===void 0&&(n=new Int32Array(t),Fv[t]=n);for(let a=0;a!==t;++a)n[a]=r.allocateTextureUnit();return n}function LA(r,t){const n=this.cache;n[0]!==t&&(r.uniform1f(this.addr,t),n[0]=t)}function OA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y)&&(r.uniform2f(this.addr,t.x,t.y),n[0]=t.x,n[1]=t.y);else{if(En(n,t))return;r.uniform2fv(this.addr,t),Tn(n,t)}}function PA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z)&&(r.uniform3f(this.addr,t.x,t.y,t.z),n[0]=t.x,n[1]=t.y,n[2]=t.z);else if(t.r!==void 0)(n[0]!==t.r||n[1]!==t.g||n[2]!==t.b)&&(r.uniform3f(this.addr,t.r,t.g,t.b),n[0]=t.r,n[1]=t.g,n[2]=t.b);else{if(En(n,t))return;r.uniform3fv(this.addr,t),Tn(n,t)}}function IA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z||n[3]!==t.w)&&(r.uniform4f(this.addr,t.x,t.y,t.z,t.w),n[0]=t.x,n[1]=t.y,n[2]=t.z,n[3]=t.w);else{if(En(n,t))return;r.uniform4fv(this.addr,t),Tn(n,t)}}function zA(r,t){const n=this.cache,a=t.elements;if(a===void 0){if(En(n,t))return;r.uniformMatrix2fv(this.addr,!1,t),Tn(n,t)}else{if(En(n,a))return;Vv.set(a),r.uniformMatrix2fv(this.addr,!1,Vv),Tn(n,a)}}function BA(r,t){const n=this.cache,a=t.elements;if(a===void 0){if(En(n,t))return;r.uniformMatrix3fv(this.addr,!1,t),Tn(n,t)}else{if(En(n,a))return;Gv.set(a),r.uniformMatrix3fv(this.addr,!1,Gv),Tn(n,a)}}function FA(r,t){const n=this.cache,a=t.elements;if(a===void 0){if(En(n,t))return;r.uniformMatrix4fv(this.addr,!1,t),Tn(n,t)}else{if(En(n,a))return;Hv.set(a),r.uniformMatrix4fv(this.addr,!1,Hv),Tn(n,a)}}function HA(r,t){const n=this.cache;n[0]!==t&&(r.uniform1i(this.addr,t),n[0]=t)}function GA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y)&&(r.uniform2i(this.addr,t.x,t.y),n[0]=t.x,n[1]=t.y);else{if(En(n,t))return;r.uniform2iv(this.addr,t),Tn(n,t)}}function VA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z)&&(r.uniform3i(this.addr,t.x,t.y,t.z),n[0]=t.x,n[1]=t.y,n[2]=t.z);else{if(En(n,t))return;r.uniform3iv(this.addr,t),Tn(n,t)}}function kA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z||n[3]!==t.w)&&(r.uniform4i(this.addr,t.x,t.y,t.z,t.w),n[0]=t.x,n[1]=t.y,n[2]=t.z,n[3]=t.w);else{if(En(n,t))return;r.uniform4iv(this.addr,t),Tn(n,t)}}function jA(r,t){const n=this.cache;n[0]!==t&&(r.uniform1ui(this.addr,t),n[0]=t)}function XA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y)&&(r.uniform2ui(this.addr,t.x,t.y),n[0]=t.x,n[1]=t.y);else{if(En(n,t))return;r.uniform2uiv(this.addr,t),Tn(n,t)}}function WA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z)&&(r.uniform3ui(this.addr,t.x,t.y,t.z),n[0]=t.x,n[1]=t.y,n[2]=t.z);else{if(En(n,t))return;r.uniform3uiv(this.addr,t),Tn(n,t)}}function qA(r,t){const n=this.cache;if(t.x!==void 0)(n[0]!==t.x||n[1]!==t.y||n[2]!==t.z||n[3]!==t.w)&&(r.uniform4ui(this.addr,t.x,t.y,t.z,t.w),n[0]=t.x,n[1]=t.y,n[2]=t.z,n[3]=t.w);else{if(En(n,t))return;r.uniform4uiv(this.addr,t),Tn(n,t)}}function YA(r,t,n){const a=this.cache,l=n.allocateTextureUnit();a[0]!==l&&(r.uniform1i(this.addr,l),a[0]=l);let c;this.type===r.SAMPLER_2D_SHADOW?(Rp.compareFunction=n.isReversedDepthBuffer()?jp:kp,c=Rp):c=$x,n.setTexture2D(t||c,l)}function ZA(r,t,n){const a=this.cache,l=n.allocateTextureUnit();a[0]!==l&&(r.uniform1i(this.addr,l),a[0]=l),n.setTexture3D(t||ey,l)}function KA(r,t,n){const a=this.cache,l=n.allocateTextureUnit();a[0]!==l&&(r.uniform1i(this.addr,l),a[0]=l),n.setTextureCube(t||ny,l)}function QA(r,t,n){const a=this.cache,l=n.allocateTextureUnit();a[0]!==l&&(r.uniform1i(this.addr,l),a[0]=l),n.setTexture2DArray(t||ty,l)}function JA(r){switch(r){case 5126:return LA;case 35664:return OA;case 35665:return PA;case 35666:return IA;case 35674:return zA;case 35675:return BA;case 35676:return FA;case 5124:case 35670:return HA;case 35667:case 35671:return GA;case 35668:case 35672:return VA;case 35669:case 35673:return kA;case 5125:return jA;case 36294:return XA;case 36295:return WA;case 36296:return qA;case 35678:case 36198:case 36298:case 36306:case 35682:return YA;case 35679:case 36299:case 36307:return ZA;case 35680:case 36300:case 36308:case 36293:return KA;case 36289:case 36303:case 36311:case 36292:return QA}}function $A(r,t){r.uniform1fv(this.addr,t)}function tw(r,t){const n=co(t,this.size,2);r.uniform2fv(this.addr,n)}function ew(r,t){const n=co(t,this.size,3);r.uniform3fv(this.addr,n)}function nw(r,t){const n=co(t,this.size,4);r.uniform4fv(this.addr,n)}function iw(r,t){const n=co(t,this.size,4);r.uniformMatrix2fv(this.addr,!1,n)}function aw(r,t){const n=co(t,this.size,9);r.uniformMatrix3fv(this.addr,!1,n)}function sw(r,t){const n=co(t,this.size,16);r.uniformMatrix4fv(this.addr,!1,n)}function rw(r,t){r.uniform1iv(this.addr,t)}function ow(r,t){r.uniform2iv(this.addr,t)}function lw(r,t){r.uniform3iv(this.addr,t)}function cw(r,t){r.uniform4iv(this.addr,t)}function uw(r,t){r.uniform1uiv(this.addr,t)}function fw(r,t){r.uniform2uiv(this.addr,t)}function dw(r,t){r.uniform3uiv(this.addr,t)}function hw(r,t){r.uniform4uiv(this.addr,t)}function pw(r,t,n){const a=this.cache,l=t.length,c=zu(n,l);En(a,c)||(r.uniform1iv(this.addr,c),Tn(a,c));let f;this.type===r.SAMPLER_2D_SHADOW?f=Rp:f=$x;for(let d=0;d!==l;++d)n.setTexture2D(t[d]||f,c[d])}function mw(r,t,n){const a=this.cache,l=t.length,c=zu(n,l);En(a,c)||(r.uniform1iv(this.addr,c),Tn(a,c));for(let f=0;f!==l;++f)n.setTexture3D(t[f]||ey,c[f])}function gw(r,t,n){const a=this.cache,l=t.length,c=zu(n,l);En(a,c)||(r.uniform1iv(this.addr,c),Tn(a,c));for(let f=0;f!==l;++f)n.setTextureCube(t[f]||ny,c[f])}function _w(r,t,n){const a=this.cache,l=t.length,c=zu(n,l);En(a,c)||(r.uniform1iv(this.addr,c),Tn(a,c));for(let f=0;f!==l;++f)n.setTexture2DArray(t[f]||ty,c[f])}function vw(r){switch(r){case 5126:return $A;case 35664:return tw;case 35665:return ew;case 35666:return nw;case 35674:return iw;case 35675:return aw;case 35676:return sw;case 5124:case 35670:return rw;case 35667:case 35671:return ow;case 35668:case 35672:return lw;case 35669:case 35673:return cw;case 5125:return uw;case 36294:return fw;case 36295:return dw;case 36296:return hw;case 35678:case 36198:case 36298:case 36306:case 35682:return pw;case 35679:case 36299:case 36307:return mw;case 35680:case 36300:case 36308:case 36293:return gw;case 36289:case 36303:case 36311:case 36292:return _w}}class xw{constructor(t,n,a){this.id=t,this.addr=a,this.cache=[],this.type=n.type,this.setValue=JA(n.type)}}class yw{constructor(t,n,a){this.id=t,this.addr=a,this.cache=[],this.type=n.type,this.size=n.size,this.setValue=vw(n.type)}}class Sw{constructor(t){this.id=t,this.seq=[],this.map={}}setValue(t,n,a){const l=this.seq;for(let c=0,f=l.length;c!==f;++c){const d=l[c];d.setValue(t,n[d.id],a)}}}const Dh=/(\w+)(\])?(\[|\.)?/g;function kv(r,t){r.seq.push(t),r.map[t.id]=t}function Mw(r,t,n){const a=r.name,l=a.length;for(Dh.lastIndex=0;;){const c=Dh.exec(a),f=Dh.lastIndex;let d=c[1];const m=c[2]==="]",h=c[3];if(m&&(d=d|0),h===void 0||h==="["&&f+2===l){kv(n,h===void 0?new xw(d,r,t):new yw(d,r,t));break}else{let _=n.map[d];_===void 0&&(_=new Sw(d),kv(n,_)),n=_}}}class xu{constructor(t,n){this.seq=[],this.map={};const a=t.getProgramParameter(n,t.ACTIVE_UNIFORMS);for(let f=0;f<a;++f){const d=t.getActiveUniform(n,f),m=t.getUniformLocation(n,d.name);Mw(d,m,this)}const l=[],c=[];for(const f of this.seq)f.type===t.SAMPLER_2D_SHADOW||f.type===t.SAMPLER_CUBE_SHADOW||f.type===t.SAMPLER_2D_ARRAY_SHADOW?l.push(f):c.push(f);l.length>0&&(this.seq=l.concat(c))}setValue(t,n,a,l){const c=this.map[n];c!==void 0&&c.setValue(t,a,l)}setOptional(t,n,a){const l=n[a];l!==void 0&&this.setValue(t,a,l)}static upload(t,n,a,l){for(let c=0,f=n.length;c!==f;++c){const d=n[c],m=a[d.id];m.needsUpdate!==!1&&d.setValue(t,m.value,l)}}static seqWithValue(t,n){const a=[];for(let l=0,c=t.length;l!==c;++l){const f=t[l];f.id in n&&a.push(f)}return a}}function jv(r,t,n){const a=r.createShader(t);return r.shaderSource(a,n),r.compileShader(a),a}const bw=37297;let Ew=0;function Tw(r,t){const n=r.split(`
`),a=[],l=Math.max(t-6,0),c=Math.min(t+6,n.length);for(let f=l;f<c;f++){const d=f+1;a.push(`${d===t?">":" "} ${d}: ${n[f]}`)}return a.join(`
`)}const Xv=new pe;function Aw(r){De._getMatrix(Xv,De.workingColorSpace,r);const t=`mat3( ${Xv.elements.map(n=>n.toFixed(4))} )`;switch(De.getTransfer(r)){case Tu:return[t,"LinearTransferOETF"];case je:return[t,"sRGBTransferOETF"];default:return ce("WebGLProgram: Unsupported color space: ",r),[t,"LinearTransferOETF"]}}function Wv(r,t,n){const a=r.getShaderParameter(t,r.COMPILE_STATUS),c=(r.getShaderInfoLog(t)||"").trim();if(a&&c==="")return"";const f=/ERROR: 0:(\d+)/.exec(c);if(f){const d=parseInt(f[1]);return n.toUpperCase()+`

`+c+`

`+Tw(r.getShaderSource(t),d)}else return c}function ww(r,t){const n=Aw(t);return[`vec4 ${r}( vec4 value ) {`,`	return ${n[1]}( vec4( value.rgb * ${n[0]}, value.a ) );`,"}"].join(`
`)}const Rw={[vx]:"Linear",[xx]:"Reinhard",[yx]:"Cineon",[Sx]:"ACESFilmic",[bx]:"AgX",[Ex]:"Neutral",[Mx]:"Custom"};function Cw(r,t){const n=Rw[t];return n===void 0?(ce("WebGLProgram: Unsupported toneMapping:",t),"vec3 "+r+"( vec3 color ) { return LinearToneMapping( color ); }"):"vec3 "+r+"( vec3 color ) { return "+n+"ToneMapping( color ); }"}const fu=new k;function Nw(){De.getLuminanceCoefficients(fu);const r=fu.x.toFixed(4),t=fu.y.toFixed(4),n=fu.z.toFixed(4);return["float luminance( const in vec3 rgb ) {",`	const vec3 weights = vec3( ${r}, ${t}, ${n} );`,"	return dot( weights, rgb );","}"].join(`
`)}function Dw(r){return[r.extensionClipCullDistance?"#extension GL_ANGLE_clip_cull_distance : require":"",r.extensionMultiDraw?"#extension GL_ANGLE_multi_draw : require":""].filter(ml).join(`
`)}function Uw(r){const t=[];for(const n in r){const a=r[n];a!==!1&&t.push("#define "+n+" "+a)}return t.join(`
`)}function Lw(r,t){const n={},a=r.getProgramParameter(t,r.ACTIVE_ATTRIBUTES);for(let l=0;l<a;l++){const c=r.getActiveAttrib(t,l),f=c.name;let d=1;c.type===r.FLOAT_MAT2&&(d=2),c.type===r.FLOAT_MAT3&&(d=3),c.type===r.FLOAT_MAT4&&(d=4),n[f]={type:c.type,location:r.getAttribLocation(t,f),locationSize:d}}return n}function ml(r){return r!==""}function qv(r,t){const n=t.numSpotLightShadows+t.numSpotLightMaps-t.numSpotLightShadowsWithMaps;return r.replace(/NUM_DIR_LIGHTS/g,t.numDirLights).replace(/NUM_SPOT_LIGHTS/g,t.numSpotLights).replace(/NUM_SPOT_LIGHT_MAPS/g,t.numSpotLightMaps).replace(/NUM_SPOT_LIGHT_COORDS/g,n).replace(/NUM_RECT_AREA_LIGHTS/g,t.numRectAreaLights).replace(/NUM_POINT_LIGHTS/g,t.numPointLights).replace(/NUM_HEMI_LIGHTS/g,t.numHemiLights).replace(/NUM_DIR_LIGHT_SHADOWS/g,t.numDirLightShadows).replace(/NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS/g,t.numSpotLightShadowsWithMaps).replace(/NUM_SPOT_LIGHT_SHADOWS/g,t.numSpotLightShadows).replace(/NUM_POINT_LIGHT_SHADOWS/g,t.numPointLightShadows)}function Yv(r,t){return r.replace(/NUM_CLIPPING_PLANES/g,t.numClippingPlanes).replace(/UNION_CLIPPING_PLANES/g,t.numClippingPlanes-t.numClipIntersection)}const Ow=/^[ \t]*#include +<([\w\d./]+)>/gm;function Cp(r){return r.replace(Ow,Iw)}const Pw=new Map;function Iw(r,t){let n=xe[t];if(n===void 0){const a=Pw.get(t);if(a!==void 0)n=xe[a],ce('WebGLRenderer: Shader chunk "%s" has been deprecated. Use "%s" instead.',t,a);else throw new Error("Can not resolve #include <"+t+">")}return Cp(n)}const zw=/#pragma unroll_loop_start\s+for\s*\(\s*int\s+i\s*=\s*(\d+)\s*;\s*i\s*<\s*(\d+)\s*;\s*i\s*\+\+\s*\)\s*{([\s\S]+?)}\s+#pragma unroll_loop_end/g;function Zv(r){return r.replace(zw,Bw)}function Bw(r,t,n,a){let l="";for(let c=parseInt(t);c<parseInt(n);c++)l+=a.replace(/\[\s*i\s*\]/g,"[ "+c+" ]").replace(/UNROLLED_LOOP_INDEX/g,c);return l}function Kv(r){let t=`precision ${r.precision} float;
	precision ${r.precision} int;
	precision ${r.precision} sampler2D;
	precision ${r.precision} samplerCube;
	precision ${r.precision} sampler3D;
	precision ${r.precision} sampler2DArray;
	precision ${r.precision} sampler2DShadow;
	precision ${r.precision} samplerCubeShadow;
	precision ${r.precision} sampler2DArrayShadow;
	precision ${r.precision} isampler2D;
	precision ${r.precision} isampler3D;
	precision ${r.precision} isamplerCube;
	precision ${r.precision} isampler2DArray;
	precision ${r.precision} usampler2D;
	precision ${r.precision} usampler3D;
	precision ${r.precision} usamplerCube;
	precision ${r.precision} usampler2DArray;
	`;return r.precision==="highp"?t+=`
#define HIGH_PRECISION`:r.precision==="mediump"?t+=`
#define MEDIUM_PRECISION`:r.precision==="lowp"&&(t+=`
#define LOW_PRECISION`),t}const Fw={[pu]:"SHADOWMAP_TYPE_PCF",[pl]:"SHADOWMAP_TYPE_VSM"};function Hw(r){return Fw[r.shadowMapType]||"SHADOWMAP_TYPE_BASIC"}const Gw={[Js]:"ENVMAP_TYPE_CUBE",[ro]:"ENVMAP_TYPE_CUBE",[Lu]:"ENVMAP_TYPE_CUBE_UV"};function Vw(r){return r.envMap===!1?"ENVMAP_TYPE_CUBE":Gw[r.envMapMode]||"ENVMAP_TYPE_CUBE"}const kw={[ro]:"ENVMAP_MODE_REFRACTION"};function jw(r){return r.envMap===!1?"ENVMAP_MODE_REFLECTION":kw[r.envMapMode]||"ENVMAP_MODE_REFLECTION"}const Xw={[_x]:"ENVMAP_BLENDING_MULTIPLY",[$M]:"ENVMAP_BLENDING_MIX",[tb]:"ENVMAP_BLENDING_ADD"};function Ww(r){return r.envMap===!1?"ENVMAP_BLENDING_NONE":Xw[r.combine]||"ENVMAP_BLENDING_NONE"}function qw(r){const t=r.envMapCubeUVHeight;if(t===null)return null;const n=Math.log2(t)-2,a=1/t;return{texelWidth:1/(3*Math.max(Math.pow(2,n),112)),texelHeight:a,maxMip:n}}function Yw(r,t,n,a){const l=r.getContext(),c=n.defines;let f=n.vertexShader,d=n.fragmentShader;const m=Hw(n),h=Vw(n),g=jw(n),_=Ww(n),v=qw(n),y=Dw(n),E=Uw(c),A=l.createProgram();let S,x,w=n.glslVersion?"#version "+n.glslVersion+`
`:"";n.isRawShaderMaterial?(S=["#define SHADER_TYPE "+n.shaderType,"#define SHADER_NAME "+n.shaderName,E].filter(ml).join(`
`),S.length>0&&(S+=`
`),x=["#define SHADER_TYPE "+n.shaderType,"#define SHADER_NAME "+n.shaderName,E].filter(ml).join(`
`),x.length>0&&(x+=`
`)):(S=[Kv(n),"#define SHADER_TYPE "+n.shaderType,"#define SHADER_NAME "+n.shaderName,E,n.extensionClipCullDistance?"#define USE_CLIP_DISTANCE":"",n.batching?"#define USE_BATCHING":"",n.batchingColor?"#define USE_BATCHING_COLOR":"",n.instancing?"#define USE_INSTANCING":"",n.instancingColor?"#define USE_INSTANCING_COLOR":"",n.instancingMorph?"#define USE_INSTANCING_MORPH":"",n.useFog&&n.fog?"#define USE_FOG":"",n.useFog&&n.fogExp2?"#define FOG_EXP2":"",n.map?"#define USE_MAP":"",n.envMap?"#define USE_ENVMAP":"",n.envMap?"#define "+g:"",n.lightMap?"#define USE_LIGHTMAP":"",n.aoMap?"#define USE_AOMAP":"",n.bumpMap?"#define USE_BUMPMAP":"",n.normalMap?"#define USE_NORMALMAP":"",n.normalMapObjectSpace?"#define USE_NORMALMAP_OBJECTSPACE":"",n.normalMapTangentSpace?"#define USE_NORMALMAP_TANGENTSPACE":"",n.displacementMap?"#define USE_DISPLACEMENTMAP":"",n.emissiveMap?"#define USE_EMISSIVEMAP":"",n.anisotropy?"#define USE_ANISOTROPY":"",n.anisotropyMap?"#define USE_ANISOTROPYMAP":"",n.clearcoatMap?"#define USE_CLEARCOATMAP":"",n.clearcoatRoughnessMap?"#define USE_CLEARCOAT_ROUGHNESSMAP":"",n.clearcoatNormalMap?"#define USE_CLEARCOAT_NORMALMAP":"",n.iridescenceMap?"#define USE_IRIDESCENCEMAP":"",n.iridescenceThicknessMap?"#define USE_IRIDESCENCE_THICKNESSMAP":"",n.specularMap?"#define USE_SPECULARMAP":"",n.specularColorMap?"#define USE_SPECULAR_COLORMAP":"",n.specularIntensityMap?"#define USE_SPECULAR_INTENSITYMAP":"",n.roughnessMap?"#define USE_ROUGHNESSMAP":"",n.metalnessMap?"#define USE_METALNESSMAP":"",n.alphaMap?"#define USE_ALPHAMAP":"",n.alphaHash?"#define USE_ALPHAHASH":"",n.transmission?"#define USE_TRANSMISSION":"",n.transmissionMap?"#define USE_TRANSMISSIONMAP":"",n.thicknessMap?"#define USE_THICKNESSMAP":"",n.sheenColorMap?"#define USE_SHEEN_COLORMAP":"",n.sheenRoughnessMap?"#define USE_SHEEN_ROUGHNESSMAP":"",n.mapUv?"#define MAP_UV "+n.mapUv:"",n.alphaMapUv?"#define ALPHAMAP_UV "+n.alphaMapUv:"",n.lightMapUv?"#define LIGHTMAP_UV "+n.lightMapUv:"",n.aoMapUv?"#define AOMAP_UV "+n.aoMapUv:"",n.emissiveMapUv?"#define EMISSIVEMAP_UV "+n.emissiveMapUv:"",n.bumpMapUv?"#define BUMPMAP_UV "+n.bumpMapUv:"",n.normalMapUv?"#define NORMALMAP_UV "+n.normalMapUv:"",n.displacementMapUv?"#define DISPLACEMENTMAP_UV "+n.displacementMapUv:"",n.metalnessMapUv?"#define METALNESSMAP_UV "+n.metalnessMapUv:"",n.roughnessMapUv?"#define ROUGHNESSMAP_UV "+n.roughnessMapUv:"",n.anisotropyMapUv?"#define ANISOTROPYMAP_UV "+n.anisotropyMapUv:"",n.clearcoatMapUv?"#define CLEARCOATMAP_UV "+n.clearcoatMapUv:"",n.clearcoatNormalMapUv?"#define CLEARCOAT_NORMALMAP_UV "+n.clearcoatNormalMapUv:"",n.clearcoatRoughnessMapUv?"#define CLEARCOAT_ROUGHNESSMAP_UV "+n.clearcoatRoughnessMapUv:"",n.iridescenceMapUv?"#define IRIDESCENCEMAP_UV "+n.iridescenceMapUv:"",n.iridescenceThicknessMapUv?"#define IRIDESCENCE_THICKNESSMAP_UV "+n.iridescenceThicknessMapUv:"",n.sheenColorMapUv?"#define SHEEN_COLORMAP_UV "+n.sheenColorMapUv:"",n.sheenRoughnessMapUv?"#define SHEEN_ROUGHNESSMAP_UV "+n.sheenRoughnessMapUv:"",n.specularMapUv?"#define SPECULARMAP_UV "+n.specularMapUv:"",n.specularColorMapUv?"#define SPECULAR_COLORMAP_UV "+n.specularColorMapUv:"",n.specularIntensityMapUv?"#define SPECULAR_INTENSITYMAP_UV "+n.specularIntensityMapUv:"",n.transmissionMapUv?"#define TRANSMISSIONMAP_UV "+n.transmissionMapUv:"",n.thicknessMapUv?"#define THICKNESSMAP_UV "+n.thicknessMapUv:"",n.vertexTangents&&n.flatShading===!1?"#define USE_TANGENT":"",n.vertexNormals?"#define HAS_NORMAL":"",n.vertexColors?"#define USE_COLOR":"",n.vertexAlphas?"#define USE_COLOR_ALPHA":"",n.vertexUv1s?"#define USE_UV1":"",n.vertexUv2s?"#define USE_UV2":"",n.vertexUv3s?"#define USE_UV3":"",n.pointsUvs?"#define USE_POINTS_UV":"",n.flatShading?"#define FLAT_SHADED":"",n.skinning?"#define USE_SKINNING":"",n.morphTargets?"#define USE_MORPHTARGETS":"",n.morphNormals&&n.flatShading===!1?"#define USE_MORPHNORMALS":"",n.morphColors?"#define USE_MORPHCOLORS":"",n.morphTargetsCount>0?"#define MORPHTARGETS_TEXTURE_STRIDE "+n.morphTextureStride:"",n.morphTargetsCount>0?"#define MORPHTARGETS_COUNT "+n.morphTargetsCount:"",n.doubleSided?"#define DOUBLE_SIDED":"",n.flipSided?"#define FLIP_SIDED":"",n.shadowMapEnabled?"#define USE_SHADOWMAP":"",n.shadowMapEnabled?"#define "+m:"",n.sizeAttenuation?"#define USE_SIZEATTENUATION":"",n.numLightProbes>0?"#define USE_LIGHT_PROBES":"",n.logarithmicDepthBuffer?"#define USE_LOGARITHMIC_DEPTH_BUFFER":"",n.reversedDepthBuffer?"#define USE_REVERSED_DEPTH_BUFFER":"","uniform mat4 modelMatrix;","uniform mat4 modelViewMatrix;","uniform mat4 projectionMatrix;","uniform mat4 viewMatrix;","uniform mat3 normalMatrix;","uniform vec3 cameraPosition;","uniform bool isOrthographic;","#ifdef USE_INSTANCING","	attribute mat4 instanceMatrix;","#endif","#ifdef USE_INSTANCING_COLOR","	attribute vec3 instanceColor;","#endif","#ifdef USE_INSTANCING_MORPH","	uniform sampler2D morphTexture;","#endif","attribute vec3 position;","attribute vec3 normal;","attribute vec2 uv;","#ifdef USE_UV1","	attribute vec2 uv1;","#endif","#ifdef USE_UV2","	attribute vec2 uv2;","#endif","#ifdef USE_UV3","	attribute vec2 uv3;","#endif","#ifdef USE_TANGENT","	attribute vec4 tangent;","#endif","#if defined( USE_COLOR_ALPHA )","	attribute vec4 color;","#elif defined( USE_COLOR )","	attribute vec3 color;","#endif","#ifdef USE_SKINNING","	attribute vec4 skinIndex;","	attribute vec4 skinWeight;","#endif",`
`].filter(ml).join(`
`),x=[Kv(n),"#define SHADER_TYPE "+n.shaderType,"#define SHADER_NAME "+n.shaderName,E,n.useFog&&n.fog?"#define USE_FOG":"",n.useFog&&n.fogExp2?"#define FOG_EXP2":"",n.alphaToCoverage?"#define ALPHA_TO_COVERAGE":"",n.map?"#define USE_MAP":"",n.matcap?"#define USE_MATCAP":"",n.envMap?"#define USE_ENVMAP":"",n.envMap?"#define "+h:"",n.envMap?"#define "+g:"",n.envMap?"#define "+_:"",v?"#define CUBEUV_TEXEL_WIDTH "+v.texelWidth:"",v?"#define CUBEUV_TEXEL_HEIGHT "+v.texelHeight:"",v?"#define CUBEUV_MAX_MIP "+v.maxMip+".0":"",n.lightMap?"#define USE_LIGHTMAP":"",n.aoMap?"#define USE_AOMAP":"",n.bumpMap?"#define USE_BUMPMAP":"",n.normalMap?"#define USE_NORMALMAP":"",n.normalMapObjectSpace?"#define USE_NORMALMAP_OBJECTSPACE":"",n.normalMapTangentSpace?"#define USE_NORMALMAP_TANGENTSPACE":"",n.packedNormalMap?"#define USE_PACKED_NORMALMAP":"",n.emissiveMap?"#define USE_EMISSIVEMAP":"",n.anisotropy?"#define USE_ANISOTROPY":"",n.anisotropyMap?"#define USE_ANISOTROPYMAP":"",n.clearcoat?"#define USE_CLEARCOAT":"",n.clearcoatMap?"#define USE_CLEARCOATMAP":"",n.clearcoatRoughnessMap?"#define USE_CLEARCOAT_ROUGHNESSMAP":"",n.clearcoatNormalMap?"#define USE_CLEARCOAT_NORMALMAP":"",n.dispersion?"#define USE_DISPERSION":"",n.iridescence?"#define USE_IRIDESCENCE":"",n.iridescenceMap?"#define USE_IRIDESCENCEMAP":"",n.iridescenceThicknessMap?"#define USE_IRIDESCENCE_THICKNESSMAP":"",n.specularMap?"#define USE_SPECULARMAP":"",n.specularColorMap?"#define USE_SPECULAR_COLORMAP":"",n.specularIntensityMap?"#define USE_SPECULAR_INTENSITYMAP":"",n.roughnessMap?"#define USE_ROUGHNESSMAP":"",n.metalnessMap?"#define USE_METALNESSMAP":"",n.alphaMap?"#define USE_ALPHAMAP":"",n.alphaTest?"#define USE_ALPHATEST":"",n.alphaHash?"#define USE_ALPHAHASH":"",n.sheen?"#define USE_SHEEN":"",n.sheenColorMap?"#define USE_SHEEN_COLORMAP":"",n.sheenRoughnessMap?"#define USE_SHEEN_ROUGHNESSMAP":"",n.transmission?"#define USE_TRANSMISSION":"",n.transmissionMap?"#define USE_TRANSMISSIONMAP":"",n.thicknessMap?"#define USE_THICKNESSMAP":"",n.vertexTangents&&n.flatShading===!1?"#define USE_TANGENT":"",n.vertexColors||n.instancingColor?"#define USE_COLOR":"",n.vertexAlphas||n.batchingColor?"#define USE_COLOR_ALPHA":"",n.vertexUv1s?"#define USE_UV1":"",n.vertexUv2s?"#define USE_UV2":"",n.vertexUv3s?"#define USE_UV3":"",n.pointsUvs?"#define USE_POINTS_UV":"",n.gradientMap?"#define USE_GRADIENTMAP":"",n.flatShading?"#define FLAT_SHADED":"",n.doubleSided?"#define DOUBLE_SIDED":"",n.flipSided?"#define FLIP_SIDED":"",n.shadowMapEnabled?"#define USE_SHADOWMAP":"",n.shadowMapEnabled?"#define "+m:"",n.premultipliedAlpha?"#define PREMULTIPLIED_ALPHA":"",n.numLightProbes>0?"#define USE_LIGHT_PROBES":"",n.numLightProbeGrids>0?"#define USE_LIGHT_PROBES_GRID":"",n.decodeVideoTexture?"#define DECODE_VIDEO_TEXTURE":"",n.decodeVideoTextureEmissive?"#define DECODE_VIDEO_TEXTURE_EMISSIVE":"",n.logarithmicDepthBuffer?"#define USE_LOGARITHMIC_DEPTH_BUFFER":"",n.reversedDepthBuffer?"#define USE_REVERSED_DEPTH_BUFFER":"","uniform mat4 viewMatrix;","uniform vec3 cameraPosition;","uniform bool isOrthographic;",n.toneMapping!==$i?"#define TONE_MAPPING":"",n.toneMapping!==$i?xe.tonemapping_pars_fragment:"",n.toneMapping!==$i?Cw("toneMapping",n.toneMapping):"",n.dithering?"#define DITHERING":"",n.opaque?"#define OPAQUE":"",xe.colorspace_pars_fragment,ww("linearToOutputTexel",n.outputColorSpace),Nw(),n.useDepthPacking?"#define DEPTH_PACKING "+n.depthPacking:"",`
`].filter(ml).join(`
`)),f=Cp(f),f=qv(f,n),f=Yv(f,n),d=Cp(d),d=qv(d,n),d=Yv(d,n),f=Zv(f),d=Zv(d),n.isRawShaderMaterial!==!0&&(w=`#version 300 es
`,S=[y,"#define attribute in","#define varying out","#define texture2D texture"].join(`
`)+`
`+S,x=["#define varying in",n.glslVersion===$_?"":"layout(location = 0) out highp vec4 pc_fragColor;",n.glslVersion===$_?"":"#define gl_FragColor pc_fragColor","#define gl_FragDepthEXT gl_FragDepth","#define texture2D texture","#define textureCube texture","#define texture2DProj textureProj","#define texture2DLodEXT textureLod","#define texture2DProjLodEXT textureProjLod","#define textureCubeLodEXT textureLod","#define texture2DGradEXT textureGrad","#define texture2DProjGradEXT textureProjGrad","#define textureCubeGradEXT textureGrad"].join(`
`)+`
`+x);const D=w+S+f,U=w+x+d,G=jv(l,l.VERTEX_SHADER,D),O=jv(l,l.FRAGMENT_SHADER,U);l.attachShader(A,G),l.attachShader(A,O),n.index0AttributeName!==void 0?l.bindAttribLocation(A,0,n.index0AttributeName):n.morphTargets===!0&&l.bindAttribLocation(A,0,"position"),l.linkProgram(A);function B(V){if(r.debug.checkShaderErrors){const $=l.getProgramInfoLog(A)||"",ht=l.getShaderInfoLog(G)||"",gt=l.getShaderInfoLog(O)||"",q=$.trim(),P=ht.trim(),F=gt.trim();let ct=!0,J=!0;if(l.getProgramParameter(A,l.LINK_STATUS)===!1)if(ct=!1,typeof r.debug.onShaderError=="function")r.debug.onShaderError(l,A,G,O);else{const xt=Wv(l,G,"vertex"),I=Wv(l,O,"fragment");Ne("THREE.WebGLProgram: Shader Error "+l.getError()+" - VALIDATE_STATUS "+l.getProgramParameter(A,l.VALIDATE_STATUS)+`

Material Name: `+V.name+`
Material Type: `+V.type+`

Program Info Log: `+q+`
`+xt+`
`+I)}else q!==""?ce("WebGLProgram: Program Info Log:",q):(P===""||F==="")&&(J=!1);J&&(V.diagnostics={runnable:ct,programLog:q,vertexShader:{log:P,prefix:S},fragmentShader:{log:F,prefix:x}})}l.deleteShader(G),l.deleteShader(O),R=new xu(l,A),z=Lw(l,A)}let R;this.getUniforms=function(){return R===void 0&&B(this),R};let z;this.getAttributes=function(){return z===void 0&&B(this),z};let K=n.rendererExtensionParallelShaderCompile===!1;return this.isReady=function(){return K===!1&&(K=l.getProgramParameter(A,bw)),K},this.destroy=function(){a.releaseStatesOfProgram(this),l.deleteProgram(A),this.program=void 0},this.type=n.shaderType,this.name=n.shaderName,this.id=Ew++,this.cacheKey=t,this.usedTimes=1,this.program=A,this.vertexShader=G,this.fragmentShader=O,this}let Zw=0;class Kw{constructor(){this.shaderCache=new Map,this.materialCache=new Map}update(t){const n=t.vertexShader,a=t.fragmentShader,l=this._getShaderStage(n),c=this._getShaderStage(a),f=this._getShaderCacheForMaterial(t);return f.has(l)===!1&&(f.add(l),l.usedTimes++),f.has(c)===!1&&(f.add(c),c.usedTimes++),this}remove(t){const n=this.materialCache.get(t);for(const a of n)a.usedTimes--,a.usedTimes===0&&this.shaderCache.delete(a.code);return this.materialCache.delete(t),this}getVertexShaderID(t){return this._getShaderStage(t.vertexShader).id}getFragmentShaderID(t){return this._getShaderStage(t.fragmentShader).id}dispose(){this.shaderCache.clear(),this.materialCache.clear()}_getShaderCacheForMaterial(t){const n=this.materialCache;let a=n.get(t);return a===void 0&&(a=new Set,n.set(t,a)),a}_getShaderStage(t){const n=this.shaderCache;let a=n.get(t);return a===void 0&&(a=new Qw(t),n.set(t,a)),a}}class Qw{constructor(t){this.id=Zw++,this.code=t,this.usedTimes=0}}function Jw(r){return r===$s||r===Mu||r===bu}function $w(r,t,n,a,l,c){const f=new qp,d=new Kw,m=new Set,h=[],g=new Map,_=a.logarithmicDepthBuffer;let v=a.precision;const y={MeshDepthMaterial:"depth",MeshDistanceMaterial:"distance",MeshNormalMaterial:"normal",MeshBasicMaterial:"basic",MeshLambertMaterial:"lambert",MeshPhongMaterial:"phong",MeshToonMaterial:"toon",MeshStandardMaterial:"physical",MeshPhysicalMaterial:"physical",MeshMatcapMaterial:"matcap",LineBasicMaterial:"basic",LineDashedMaterial:"dashed",PointsMaterial:"points",ShadowMaterial:"shadow",SpriteMaterial:"sprite"};function E(R){return m.add(R),R===0?"uv":`uv${R}`}function A(R,z,K,V,$,ht){const gt=V.fog,q=$.geometry,P=R.isMeshStandardMaterial||R.isMeshLambertMaterial||R.isMeshPhongMaterial?V.environment:null,F=R.isMeshStandardMaterial||R.isMeshLambertMaterial&&!R.envMap||R.isMeshPhongMaterial&&!R.envMap,ct=t.get(R.envMap||P,F),J=ct&&ct.mapping===Lu?ct.image.height:null,xt=y[R.type];R.precision!==null&&(v=a.getMaxPrecision(R.precision),v!==R.precision&&ce("WebGLProgram.getParameters:",R.precision,"not supported, using",v,"instead."));const I=q.morphAttributes.position||q.morphAttributes.normal||q.morphAttributes.color,Q=I!==void 0?I.length:0;let Mt=0;q.morphAttributes.position!==void 0&&(Mt=1),q.morphAttributes.normal!==void 0&&(Mt=2),q.morphAttributes.color!==void 0&&(Mt=3);let Rt,wt,st,bt;if(xt){const ue=Ki[xt];Rt=ue.vertexShader,wt=ue.fragmentShader}else Rt=R.vertexShader,wt=R.fragmentShader,d.update(R),st=d.getVertexShaderID(R),bt=d.getFragmentShaderID(R);const Tt=r.getRenderTarget(),Wt=r.state.buffers.depth.getReversed(),re=$.isInstancedMesh===!0,ie=$.isBatchedMesh===!0,Nt=!!R.map,Ht=!!R.matcap,Ut=!!ct,Gt=!!R.aoMap,ne=!!R.lightMap,Me=!!R.bumpMap,le=!!R.normalMap,Ue=!!R.displacementMap,W=!!R.emissiveMap,We=!!R.metalnessMap,ye=!!R.roughnessMap,qe=R.anisotropy>0,Dt=R.clearcoat>0,an=R.dispersion>0,L=R.iridescence>0,T=R.sheen>0,tt=R.transmission>0,yt=qe&&!!R.anisotropyMap,At=Dt&&!!R.clearcoatMap,Lt=Dt&&!!R.clearcoatNormalMap,zt=Dt&&!!R.clearcoatRoughnessMap,dt=L&&!!R.iridescenceMap,pt=L&&!!R.iridescenceThicknessMap,Bt=T&&!!R.sheenColorMap,Ft=T&&!!R.sheenRoughnessMap,Pt=!!R.specularMap,Ot=!!R.specularColorMap,fe=!!R.specularIntensityMap,de=tt&&!!R.transmissionMap,be=tt&&!!R.thicknessMap,j=!!R.gradientMap,Ct=!!R.alphaMap,_t=R.alphaTest>0,jt=!!R.alphaHash,It=!!R.extensions;let Et=$i;R.toneMapped&&(Tt===null||Tt.isXRRenderTarget===!0)&&(Et=r.toneMapping);const Jt={shaderID:xt,shaderType:R.type,shaderName:R.name,vertexShader:Rt,fragmentShader:wt,defines:R.defines,customVertexShaderID:st,customFragmentShaderID:bt,isRawShaderMaterial:R.isRawShaderMaterial===!0,glslVersion:R.glslVersion,precision:v,batching:ie,batchingColor:ie&&$._colorsTexture!==null,instancing:re,instancingColor:re&&$.instanceColor!==null,instancingMorph:re&&$.morphTexture!==null,outputColorSpace:Tt===null?r.outputColorSpace:Tt.isXRRenderTarget===!0?Tt.texture.colorSpace:De.workingColorSpace,alphaToCoverage:!!R.alphaToCoverage,map:Nt,matcap:Ht,envMap:Ut,envMapMode:Ut&&ct.mapping,envMapCubeUVHeight:J,aoMap:Gt,lightMap:ne,bumpMap:Me,normalMap:le,displacementMap:Ue,emissiveMap:W,normalMapObjectSpace:le&&R.normalMapType===ib,normalMapTangentSpace:le&&R.normalMapType===bp,packedNormalMap:le&&R.normalMapType===bp&&Jw(R.normalMap.format),metalnessMap:We,roughnessMap:ye,anisotropy:qe,anisotropyMap:yt,clearcoat:Dt,clearcoatMap:At,clearcoatNormalMap:Lt,clearcoatRoughnessMap:zt,dispersion:an,iridescence:L,iridescenceMap:dt,iridescenceThicknessMap:pt,sheen:T,sheenColorMap:Bt,sheenRoughnessMap:Ft,specularMap:Pt,specularColorMap:Ot,specularIntensityMap:fe,transmission:tt,transmissionMap:de,thicknessMap:be,gradientMap:j,opaque:R.transparent===!1&&R.blending===io&&R.alphaToCoverage===!1,alphaMap:Ct,alphaTest:_t,alphaHash:jt,combine:R.combine,mapUv:Nt&&E(R.map.channel),aoMapUv:Gt&&E(R.aoMap.channel),lightMapUv:ne&&E(R.lightMap.channel),bumpMapUv:Me&&E(R.bumpMap.channel),normalMapUv:le&&E(R.normalMap.channel),displacementMapUv:Ue&&E(R.displacementMap.channel),emissiveMapUv:W&&E(R.emissiveMap.channel),metalnessMapUv:We&&E(R.metalnessMap.channel),roughnessMapUv:ye&&E(R.roughnessMap.channel),anisotropyMapUv:yt&&E(R.anisotropyMap.channel),clearcoatMapUv:At&&E(R.clearcoatMap.channel),clearcoatNormalMapUv:Lt&&E(R.clearcoatNormalMap.channel),clearcoatRoughnessMapUv:zt&&E(R.clearcoatRoughnessMap.channel),iridescenceMapUv:dt&&E(R.iridescenceMap.channel),iridescenceThicknessMapUv:pt&&E(R.iridescenceThicknessMap.channel),sheenColorMapUv:Bt&&E(R.sheenColorMap.channel),sheenRoughnessMapUv:Ft&&E(R.sheenRoughnessMap.channel),specularMapUv:Pt&&E(R.specularMap.channel),specularColorMapUv:Ot&&E(R.specularColorMap.channel),specularIntensityMapUv:fe&&E(R.specularIntensityMap.channel),transmissionMapUv:de&&E(R.transmissionMap.channel),thicknessMapUv:be&&E(R.thicknessMap.channel),alphaMapUv:Ct&&E(R.alphaMap.channel),vertexTangents:!!q.attributes.tangent&&(le||qe),vertexNormals:!!q.attributes.normal,vertexColors:R.vertexColors,vertexAlphas:R.vertexColors===!0&&!!q.attributes.color&&q.attributes.color.itemSize===4,pointsUvs:$.isPoints===!0&&!!q.attributes.uv&&(Nt||Ct),fog:!!gt,useFog:R.fog===!0,fogExp2:!!gt&&gt.isFogExp2,flatShading:R.wireframe===!1&&(R.flatShading===!0||q.attributes.normal===void 0&&le===!1&&(R.isMeshLambertMaterial||R.isMeshPhongMaterial||R.isMeshStandardMaterial||R.isMeshPhysicalMaterial)),sizeAttenuation:R.sizeAttenuation===!0,logarithmicDepthBuffer:_,reversedDepthBuffer:Wt,skinning:$.isSkinnedMesh===!0,morphTargets:q.morphAttributes.position!==void 0,morphNormals:q.morphAttributes.normal!==void 0,morphColors:q.morphAttributes.color!==void 0,morphTargetsCount:Q,morphTextureStride:Mt,numDirLights:z.directional.length,numPointLights:z.point.length,numSpotLights:z.spot.length,numSpotLightMaps:z.spotLightMap.length,numRectAreaLights:z.rectArea.length,numHemiLights:z.hemi.length,numDirLightShadows:z.directionalShadowMap.length,numPointLightShadows:z.pointShadowMap.length,numSpotLightShadows:z.spotShadowMap.length,numSpotLightShadowsWithMaps:z.numSpotLightShadowsWithMaps,numLightProbes:z.numLightProbes,numLightProbeGrids:ht.length,numClippingPlanes:c.numPlanes,numClipIntersection:c.numIntersection,dithering:R.dithering,shadowMapEnabled:r.shadowMap.enabled&&K.length>0,shadowMapType:r.shadowMap.type,toneMapping:Et,decodeVideoTexture:Nt&&R.map.isVideoTexture===!0&&De.getTransfer(R.map.colorSpace)===je,decodeVideoTextureEmissive:W&&R.emissiveMap.isVideoTexture===!0&&De.getTransfer(R.emissiveMap.colorSpace)===je,premultipliedAlpha:R.premultipliedAlpha,doubleSided:R.side===Aa,flipSided:R.side===ti,useDepthPacking:R.depthPacking>=0,depthPacking:R.depthPacking||0,index0AttributeName:R.index0AttributeName,extensionClipCullDistance:It&&R.extensions.clipCullDistance===!0&&n.has("WEBGL_clip_cull_distance"),extensionMultiDraw:(It&&R.extensions.multiDraw===!0||ie)&&n.has("WEBGL_multi_draw"),rendererExtensionParallelShaderCompile:n.has("KHR_parallel_shader_compile"),customProgramCacheKey:R.customProgramCacheKey()};return Jt.vertexUv1s=m.has(1),Jt.vertexUv2s=m.has(2),Jt.vertexUv3s=m.has(3),m.clear(),Jt}function S(R){const z=[];if(R.shaderID?z.push(R.shaderID):(z.push(R.customVertexShaderID),z.push(R.customFragmentShaderID)),R.defines!==void 0)for(const K in R.defines)z.push(K),z.push(R.defines[K]);return R.isRawShaderMaterial===!1&&(x(z,R),w(z,R),z.push(r.outputColorSpace)),z.push(R.customProgramCacheKey),z.join()}function x(R,z){R.push(z.precision),R.push(z.outputColorSpace),R.push(z.envMapMode),R.push(z.envMapCubeUVHeight),R.push(z.mapUv),R.push(z.alphaMapUv),R.push(z.lightMapUv),R.push(z.aoMapUv),R.push(z.bumpMapUv),R.push(z.normalMapUv),R.push(z.displacementMapUv),R.push(z.emissiveMapUv),R.push(z.metalnessMapUv),R.push(z.roughnessMapUv),R.push(z.anisotropyMapUv),R.push(z.clearcoatMapUv),R.push(z.clearcoatNormalMapUv),R.push(z.clearcoatRoughnessMapUv),R.push(z.iridescenceMapUv),R.push(z.iridescenceThicknessMapUv),R.push(z.sheenColorMapUv),R.push(z.sheenRoughnessMapUv),R.push(z.specularMapUv),R.push(z.specularColorMapUv),R.push(z.specularIntensityMapUv),R.push(z.transmissionMapUv),R.push(z.thicknessMapUv),R.push(z.combine),R.push(z.fogExp2),R.push(z.sizeAttenuation),R.push(z.morphTargetsCount),R.push(z.morphAttributeCount),R.push(z.numDirLights),R.push(z.numPointLights),R.push(z.numSpotLights),R.push(z.numSpotLightMaps),R.push(z.numHemiLights),R.push(z.numRectAreaLights),R.push(z.numDirLightShadows),R.push(z.numPointLightShadows),R.push(z.numSpotLightShadows),R.push(z.numSpotLightShadowsWithMaps),R.push(z.numLightProbes),R.push(z.shadowMapType),R.push(z.toneMapping),R.push(z.numClippingPlanes),R.push(z.numClipIntersection),R.push(z.depthPacking)}function w(R,z){f.disableAll(),z.instancing&&f.enable(0),z.instancingColor&&f.enable(1),z.instancingMorph&&f.enable(2),z.matcap&&f.enable(3),z.envMap&&f.enable(4),z.normalMapObjectSpace&&f.enable(5),z.normalMapTangentSpace&&f.enable(6),z.clearcoat&&f.enable(7),z.iridescence&&f.enable(8),z.alphaTest&&f.enable(9),z.vertexColors&&f.enable(10),z.vertexAlphas&&f.enable(11),z.vertexUv1s&&f.enable(12),z.vertexUv2s&&f.enable(13),z.vertexUv3s&&f.enable(14),z.vertexTangents&&f.enable(15),z.anisotropy&&f.enable(16),z.alphaHash&&f.enable(17),z.batching&&f.enable(18),z.dispersion&&f.enable(19),z.batchingColor&&f.enable(20),z.gradientMap&&f.enable(21),z.packedNormalMap&&f.enable(22),z.vertexNormals&&f.enable(23),R.push(f.mask),f.disableAll(),z.fog&&f.enable(0),z.useFog&&f.enable(1),z.flatShading&&f.enable(2),z.logarithmicDepthBuffer&&f.enable(3),z.reversedDepthBuffer&&f.enable(4),z.skinning&&f.enable(5),z.morphTargets&&f.enable(6),z.morphNormals&&f.enable(7),z.morphColors&&f.enable(8),z.premultipliedAlpha&&f.enable(9),z.shadowMapEnabled&&f.enable(10),z.doubleSided&&f.enable(11),z.flipSided&&f.enable(12),z.useDepthPacking&&f.enable(13),z.dithering&&f.enable(14),z.transmission&&f.enable(15),z.sheen&&f.enable(16),z.opaque&&f.enable(17),z.pointsUvs&&f.enable(18),z.decodeVideoTexture&&f.enable(19),z.decodeVideoTextureEmissive&&f.enable(20),z.alphaToCoverage&&f.enable(21),z.numLightProbeGrids>0&&f.enable(22),R.push(f.mask)}function D(R){const z=y[R.type];let K;if(z){const V=Ki[z];K=vE.clone(V.uniforms)}else K=R.uniforms;return K}function U(R,z){let K=g.get(z);return K!==void 0?++K.usedTimes:(K=new Yw(r,z,R,l),h.push(K),g.set(z,K)),K}function G(R){if(--R.usedTimes===0){const z=h.indexOf(R);h[z]=h[h.length-1],h.pop(),g.delete(R.cacheKey),R.destroy()}}function O(R){d.remove(R)}function B(){d.dispose()}return{getParameters:A,getProgramCacheKey:S,getUniforms:D,acquireProgram:U,releaseProgram:G,releaseShaderCache:O,programs:h,dispose:B}}function tR(){let r=new WeakMap;function t(f){return r.has(f)}function n(f){let d=r.get(f);return d===void 0&&(d={},r.set(f,d)),d}function a(f){r.delete(f)}function l(f,d,m){r.get(f)[d]=m}function c(){r=new WeakMap}return{has:t,get:n,remove:a,update:l,dispose:c}}function eR(r,t){return r.groupOrder!==t.groupOrder?r.groupOrder-t.groupOrder:r.renderOrder!==t.renderOrder?r.renderOrder-t.renderOrder:r.material.id!==t.material.id?r.material.id-t.material.id:r.materialVariant!==t.materialVariant?r.materialVariant-t.materialVariant:r.z!==t.z?r.z-t.z:r.id-t.id}function Qv(r,t){return r.groupOrder!==t.groupOrder?r.groupOrder-t.groupOrder:r.renderOrder!==t.renderOrder?r.renderOrder-t.renderOrder:r.z!==t.z?t.z-r.z:r.id-t.id}function Jv(){const r=[];let t=0;const n=[],a=[],l=[];function c(){t=0,n.length=0,a.length=0,l.length=0}function f(v){let y=0;return v.isInstancedMesh&&(y+=2),v.isSkinnedMesh&&(y+=1),y}function d(v,y,E,A,S,x){let w=r[t];return w===void 0?(w={id:v.id,object:v,geometry:y,material:E,materialVariant:f(v),groupOrder:A,renderOrder:v.renderOrder,z:S,group:x},r[t]=w):(w.id=v.id,w.object=v,w.geometry=y,w.material=E,w.materialVariant=f(v),w.groupOrder=A,w.renderOrder=v.renderOrder,w.z=S,w.group=x),t++,w}function m(v,y,E,A,S,x){const w=d(v,y,E,A,S,x);E.transmission>0?a.push(w):E.transparent===!0?l.push(w):n.push(w)}function h(v,y,E,A,S,x){const w=d(v,y,E,A,S,x);E.transmission>0?a.unshift(w):E.transparent===!0?l.unshift(w):n.unshift(w)}function g(v,y){n.length>1&&n.sort(v||eR),a.length>1&&a.sort(y||Qv),l.length>1&&l.sort(y||Qv)}function _(){for(let v=t,y=r.length;v<y;v++){const E=r[v];if(E.id===null)break;E.id=null,E.object=null,E.geometry=null,E.material=null,E.group=null}}return{opaque:n,transmissive:a,transparent:l,init:c,push:m,unshift:h,finish:_,sort:g}}function nR(){let r=new WeakMap;function t(a,l){const c=r.get(a);let f;return c===void 0?(f=new Jv,r.set(a,[f])):l>=c.length?(f=new Jv,c.push(f)):f=c[l],f}function n(){r=new WeakMap}return{get:t,dispose:n}}function iR(){const r={};return{get:function(t){if(r[t.id]!==void 0)return r[t.id];let n;switch(t.type){case"DirectionalLight":n={direction:new k,color:new _e};break;case"SpotLight":n={position:new k,direction:new k,color:new _e,distance:0,coneCos:0,penumbraCos:0,decay:0};break;case"PointLight":n={position:new k,color:new _e,distance:0,decay:0};break;case"HemisphereLight":n={direction:new k,skyColor:new _e,groundColor:new _e};break;case"RectAreaLight":n={color:new _e,position:new k,halfWidth:new k,halfHeight:new k};break}return r[t.id]=n,n}}}function aR(){const r={};return{get:function(t){if(r[t.id]!==void 0)return r[t.id];let n;switch(t.type){case"DirectionalLight":n={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new ee};break;case"SpotLight":n={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new ee};break;case"PointLight":n={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new ee,shadowCameraNear:1,shadowCameraFar:1e3};break}return r[t.id]=n,n}}}let sR=0;function rR(r,t){return(t.castShadow?2:0)-(r.castShadow?2:0)+(t.map?1:0)-(r.map?1:0)}function oR(r){const t=new iR,n=aR(),a={version:0,hash:{directionalLength:-1,pointLength:-1,spotLength:-1,rectAreaLength:-1,hemiLength:-1,numDirectionalShadows:-1,numPointShadows:-1,numSpotShadows:-1,numSpotMaps:-1,numLightProbes:-1},ambient:[0,0,0],probe:[],directional:[],directionalShadow:[],directionalShadowMap:[],directionalShadowMatrix:[],spot:[],spotLightMap:[],spotShadow:[],spotShadowMap:[],spotLightMatrix:[],rectArea:[],rectAreaLTC1:null,rectAreaLTC2:null,point:[],pointShadow:[],pointShadowMap:[],pointShadowMatrix:[],hemi:[],numSpotLightShadowsWithMaps:0,numLightProbes:0};for(let h=0;h<9;h++)a.probe.push(new k);const l=new k,c=new tn,f=new tn;function d(h){let g=0,_=0,v=0;for(let z=0;z<9;z++)a.probe[z].set(0,0,0);let y=0,E=0,A=0,S=0,x=0,w=0,D=0,U=0,G=0,O=0,B=0;h.sort(rR);for(let z=0,K=h.length;z<K;z++){const V=h[z],$=V.color,ht=V.intensity,gt=V.distance;let q=null;if(V.shadow&&V.shadow.map&&(V.shadow.map.texture.format===$s?q=V.shadow.map.texture:q=V.shadow.map.depthTexture||V.shadow.map.texture),V.isAmbientLight)g+=$.r*ht,_+=$.g*ht,v+=$.b*ht;else if(V.isLightProbe){for(let P=0;P<9;P++)a.probe[P].addScaledVector(V.sh.coefficients[P],ht);B++}else if(V.isDirectionalLight){const P=t.get(V);if(P.color.copy(V.color).multiplyScalar(V.intensity),V.castShadow){const F=V.shadow,ct=n.get(V);ct.shadowIntensity=F.intensity,ct.shadowBias=F.bias,ct.shadowNormalBias=F.normalBias,ct.shadowRadius=F.radius,ct.shadowMapSize=F.mapSize,a.directionalShadow[y]=ct,a.directionalShadowMap[y]=q,a.directionalShadowMatrix[y]=V.shadow.matrix,w++}a.directional[y]=P,y++}else if(V.isSpotLight){const P=t.get(V);P.position.setFromMatrixPosition(V.matrixWorld),P.color.copy($).multiplyScalar(ht),P.distance=gt,P.coneCos=Math.cos(V.angle),P.penumbraCos=Math.cos(V.angle*(1-V.penumbra)),P.decay=V.decay,a.spot[A]=P;const F=V.shadow;if(V.map&&(a.spotLightMap[G]=V.map,G++,F.updateMatrices(V),V.castShadow&&O++),a.spotLightMatrix[A]=F.matrix,V.castShadow){const ct=n.get(V);ct.shadowIntensity=F.intensity,ct.shadowBias=F.bias,ct.shadowNormalBias=F.normalBias,ct.shadowRadius=F.radius,ct.shadowMapSize=F.mapSize,a.spotShadow[A]=ct,a.spotShadowMap[A]=q,U++}A++}else if(V.isRectAreaLight){const P=t.get(V);P.color.copy($).multiplyScalar(ht),P.halfWidth.set(V.width*.5,0,0),P.halfHeight.set(0,V.height*.5,0),a.rectArea[S]=P,S++}else if(V.isPointLight){const P=t.get(V);if(P.color.copy(V.color).multiplyScalar(V.intensity),P.distance=V.distance,P.decay=V.decay,V.castShadow){const F=V.shadow,ct=n.get(V);ct.shadowIntensity=F.intensity,ct.shadowBias=F.bias,ct.shadowNormalBias=F.normalBias,ct.shadowRadius=F.radius,ct.shadowMapSize=F.mapSize,ct.shadowCameraNear=F.camera.near,ct.shadowCameraFar=F.camera.far,a.pointShadow[E]=ct,a.pointShadowMap[E]=q,a.pointShadowMatrix[E]=V.shadow.matrix,D++}a.point[E]=P,E++}else if(V.isHemisphereLight){const P=t.get(V);P.skyColor.copy(V.color).multiplyScalar(ht),P.groundColor.copy(V.groundColor).multiplyScalar(ht),a.hemi[x]=P,x++}}S>0&&(r.has("OES_texture_float_linear")===!0?(a.rectAreaLTC1=Xt.LTC_FLOAT_1,a.rectAreaLTC2=Xt.LTC_FLOAT_2):(a.rectAreaLTC1=Xt.LTC_HALF_1,a.rectAreaLTC2=Xt.LTC_HALF_2)),a.ambient[0]=g,a.ambient[1]=_,a.ambient[2]=v;const R=a.hash;(R.directionalLength!==y||R.pointLength!==E||R.spotLength!==A||R.rectAreaLength!==S||R.hemiLength!==x||R.numDirectionalShadows!==w||R.numPointShadows!==D||R.numSpotShadows!==U||R.numSpotMaps!==G||R.numLightProbes!==B)&&(a.directional.length=y,a.spot.length=A,a.rectArea.length=S,a.point.length=E,a.hemi.length=x,a.directionalShadow.length=w,a.directionalShadowMap.length=w,a.pointShadow.length=D,a.pointShadowMap.length=D,a.spotShadow.length=U,a.spotShadowMap.length=U,a.directionalShadowMatrix.length=w,a.pointShadowMatrix.length=D,a.spotLightMatrix.length=U+G-O,a.spotLightMap.length=G,a.numSpotLightShadowsWithMaps=O,a.numLightProbes=B,R.directionalLength=y,R.pointLength=E,R.spotLength=A,R.rectAreaLength=S,R.hemiLength=x,R.numDirectionalShadows=w,R.numPointShadows=D,R.numSpotShadows=U,R.numSpotMaps=G,R.numLightProbes=B,a.version=sR++)}function m(h,g){let _=0,v=0,y=0,E=0,A=0;const S=g.matrixWorldInverse;for(let x=0,w=h.length;x<w;x++){const D=h[x];if(D.isDirectionalLight){const U=a.directional[_];U.direction.setFromMatrixPosition(D.matrixWorld),l.setFromMatrixPosition(D.target.matrixWorld),U.direction.sub(l),U.direction.transformDirection(S),_++}else if(D.isSpotLight){const U=a.spot[y];U.position.setFromMatrixPosition(D.matrixWorld),U.position.applyMatrix4(S),U.direction.setFromMatrixPosition(D.matrixWorld),l.setFromMatrixPosition(D.target.matrixWorld),U.direction.sub(l),U.direction.transformDirection(S),y++}else if(D.isRectAreaLight){const U=a.rectArea[E];U.position.setFromMatrixPosition(D.matrixWorld),U.position.applyMatrix4(S),f.identity(),c.copy(D.matrixWorld),c.premultiply(S),f.extractRotation(c),U.halfWidth.set(D.width*.5,0,0),U.halfHeight.set(0,D.height*.5,0),U.halfWidth.applyMatrix4(f),U.halfHeight.applyMatrix4(f),E++}else if(D.isPointLight){const U=a.point[v];U.position.setFromMatrixPosition(D.matrixWorld),U.position.applyMatrix4(S),v++}else if(D.isHemisphereLight){const U=a.hemi[A];U.direction.setFromMatrixPosition(D.matrixWorld),U.direction.transformDirection(S),A++}}}return{setup:d,setupView:m,state:a}}function $v(r){const t=new oR(r),n=[],a=[],l=[];function c(v){_.camera=v,n.length=0,a.length=0,l.length=0}function f(v){n.push(v)}function d(v){a.push(v)}function m(v){l.push(v)}function h(){t.setup(n)}function g(v){t.setupView(n,v)}const _={lightsArray:n,shadowsArray:a,lightProbeGridArray:l,camera:null,lights:t,transmissionRenderTarget:{},textureUnits:0};return{init:c,state:_,setupLights:h,setupLightsView:g,pushLight:f,pushShadow:d,pushLightProbeGrid:m}}function lR(r){let t=new WeakMap;function n(l,c=0){const f=t.get(l);let d;return f===void 0?(d=new $v(r),t.set(l,[d])):c>=f.length?(d=new $v(r),f.push(d)):d=f[c],d}function a(){t=new WeakMap}return{get:n,dispose:a}}const cR=`void main() {
	gl_Position = vec4( position, 1.0 );
}`,uR=`uniform sampler2D shadow_pass;
uniform vec2 resolution;
uniform float radius;
void main() {
	const float samples = float( VSM_SAMPLES );
	float mean = 0.0;
	float squared_mean = 0.0;
	float uvStride = samples <= 1.0 ? 0.0 : 2.0 / ( samples - 1.0 );
	float uvStart = samples <= 1.0 ? 0.0 : - 1.0;
	for ( float i = 0.0; i < samples; i ++ ) {
		float uvOffset = uvStart + i * uvStride;
		#ifdef HORIZONTAL_PASS
			vec2 distribution = texture2D( shadow_pass, ( gl_FragCoord.xy + vec2( uvOffset, 0.0 ) * radius ) / resolution ).rg;
			mean += distribution.x;
			squared_mean += distribution.y * distribution.y + distribution.x * distribution.x;
		#else
			float depth = texture2D( shadow_pass, ( gl_FragCoord.xy + vec2( 0.0, uvOffset ) * radius ) / resolution ).r;
			mean += depth;
			squared_mean += depth * depth;
		#endif
	}
	mean = mean / samples;
	squared_mean = squared_mean / samples;
	float std_dev = sqrt( max( 0.0, squared_mean - mean * mean ) );
	gl_FragColor = vec4( mean, std_dev, 0.0, 1.0 );
}`,fR=[new k(1,0,0),new k(-1,0,0),new k(0,1,0),new k(0,-1,0),new k(0,0,1),new k(0,0,-1)],dR=[new k(0,-1,0),new k(0,-1,0),new k(0,0,1),new k(0,0,-1),new k(0,-1,0),new k(0,-1,0)],tx=new tn,hl=new k,Uh=new k;function hR(r,t,n){let a=new Zp;const l=new ee,c=new ee,f=new fn,d=new ME,m=new bE,h={},g=n.maxTextureSize,_={[vs]:ti,[ti]:vs,[Aa]:Aa},v=new na({defines:{VSM_SAMPLES:8},uniforms:{shadow_pass:{value:null},resolution:{value:new ee},radius:{value:4}},vertexShader:cR,fragmentShader:uR}),y=v.clone();y.defines.HORIZONTAL_PASS=1;const E=new qn;E.setAttribute("position",new Ci(new Float32Array([-1,-1,.5,3,-1,.5,-1,3,.5]),3));const A=new Gn(E,v),S=this;this.enabled=!1,this.autoUpdate=!0,this.needsUpdate=!1,this.type=pu;let x=this.type;this.render=function(O,B,R){if(S.enabled===!1||S.autoUpdate===!1&&S.needsUpdate===!1||O.length===0)return;this.type===OM&&(ce("WebGLShadowMap: PCFSoftShadowMap has been deprecated. Using PCFShadowMap instead."),this.type=pu);const z=r.getRenderTarget(),K=r.getActiveCubeFace(),V=r.getActiveMipmapLevel(),$=r.state;$.setBlending(Ra),$.buffers.depth.getReversed()===!0?$.buffers.color.setClear(0,0,0,0):$.buffers.color.setClear(1,1,1,1),$.buffers.depth.setTest(!0),$.setScissorTest(!1);const ht=x!==this.type;ht&&B.traverse(function(gt){gt.material&&(Array.isArray(gt.material)?gt.material.forEach(q=>q.needsUpdate=!0):gt.material.needsUpdate=!0)});for(let gt=0,q=O.length;gt<q;gt++){const P=O[gt],F=P.shadow;if(F===void 0){ce("WebGLShadowMap:",P,"has no shadow.");continue}if(F.autoUpdate===!1&&F.needsUpdate===!1)continue;l.copy(F.mapSize);const ct=F.getFrameExtents();l.multiply(ct),c.copy(F.mapSize),(l.x>g||l.y>g)&&(l.x>g&&(c.x=Math.floor(g/ct.x),l.x=c.x*ct.x,F.mapSize.x=c.x),l.y>g&&(c.y=Math.floor(g/ct.y),l.y=c.y*ct.y,F.mapSize.y=c.y));const J=r.state.buffers.depth.getReversed();if(F.camera._reversedDepth=J,F.map===null||ht===!0){if(F.map!==null&&(F.map.depthTexture!==null&&(F.map.depthTexture.dispose(),F.map.depthTexture=null),F.map.dispose()),this.type===pl){if(P.isPointLight){ce("WebGLShadowMap: VSM shadow maps are not supported for PointLights. Use PCF or BasicShadowMap instead.");continue}F.map=new ta(l.x,l.y,{format:$s,type:Da,minFilter:Sn,magFilter:Sn,generateMipmaps:!1}),F.map.texture.name=P.name+".shadowMap",F.map.depthTexture=new oo(l.x,l.y,Qi),F.map.depthTexture.name=P.name+".shadowMapDepth",F.map.depthTexture.format=Ua,F.map.depthTexture.compareFunction=null,F.map.depthTexture.minFilter=Pn,F.map.depthTexture.magFilter=Pn}else P.isPointLight?(F.map=new Jx(l.x),F.map.depthTexture=new eE(l.x,ea)):(F.map=new ta(l.x,l.y),F.map.depthTexture=new oo(l.x,l.y,ea)),F.map.depthTexture.name=P.name+".shadowMap",F.map.depthTexture.format=Ua,this.type===pu?(F.map.depthTexture.compareFunction=J?jp:kp,F.map.depthTexture.minFilter=Sn,F.map.depthTexture.magFilter=Sn):(F.map.depthTexture.compareFunction=null,F.map.depthTexture.minFilter=Pn,F.map.depthTexture.magFilter=Pn);F.camera.updateProjectionMatrix()}const xt=F.map.isWebGLCubeRenderTarget?6:1;for(let I=0;I<xt;I++){if(F.map.isWebGLCubeRenderTarget)r.setRenderTarget(F.map,I),r.clear();else{I===0&&(r.setRenderTarget(F.map),r.clear());const Q=F.getViewport(I);f.set(c.x*Q.x,c.y*Q.y,c.x*Q.z,c.y*Q.w),$.viewport(f)}if(P.isPointLight){const Q=F.camera,Mt=F.matrix,Rt=P.distance||Q.far;Rt!==Q.far&&(Q.far=Rt,Q.updateProjectionMatrix()),hl.setFromMatrixPosition(P.matrixWorld),Q.position.copy(hl),Uh.copy(Q.position),Uh.add(fR[I]),Q.up.copy(dR[I]),Q.lookAt(Uh),Q.updateMatrixWorld(),Mt.makeTranslation(-hl.x,-hl.y,-hl.z),tx.multiplyMatrices(Q.projectionMatrix,Q.matrixWorldInverse),F._frustum.setFromProjectionMatrix(tx,Q.coordinateSystem,Q.reversedDepth)}else F.updateMatrices(P);a=F.getFrustum(),U(B,R,F.camera,P,this.type)}F.isPointLightShadow!==!0&&this.type===pl&&w(F,R),F.needsUpdate=!1}x=this.type,S.needsUpdate=!1,r.setRenderTarget(z,K,V)};function w(O,B){const R=t.update(A);v.defines.VSM_SAMPLES!==O.blurSamples&&(v.defines.VSM_SAMPLES=O.blurSamples,y.defines.VSM_SAMPLES=O.blurSamples,v.needsUpdate=!0,y.needsUpdate=!0),O.mapPass===null&&(O.mapPass=new ta(l.x,l.y,{format:$s,type:Da})),v.uniforms.shadow_pass.value=O.map.depthTexture,v.uniforms.resolution.value=O.mapSize,v.uniforms.radius.value=O.radius,r.setRenderTarget(O.mapPass),r.clear(),r.renderBufferDirect(B,null,R,v,A,null),y.uniforms.shadow_pass.value=O.mapPass.texture,y.uniforms.resolution.value=O.mapSize,y.uniforms.radius.value=O.radius,r.setRenderTarget(O.map),r.clear(),r.renderBufferDirect(B,null,R,y,A,null)}function D(O,B,R,z){let K=null;const V=R.isPointLight===!0?O.customDistanceMaterial:O.customDepthMaterial;if(V!==void 0)K=V;else if(K=R.isPointLight===!0?m:d,r.localClippingEnabled&&B.clipShadows===!0&&Array.isArray(B.clippingPlanes)&&B.clippingPlanes.length!==0||B.displacementMap&&B.displacementScale!==0||B.alphaMap&&B.alphaTest>0||B.map&&B.alphaTest>0||B.alphaToCoverage===!0){const $=K.uuid,ht=B.uuid;let gt=h[$];gt===void 0&&(gt={},h[$]=gt);let q=gt[ht];q===void 0&&(q=K.clone(),gt[ht]=q,B.addEventListener("dispose",G)),K=q}if(K.visible=B.visible,K.wireframe=B.wireframe,z===pl?K.side=B.shadowSide!==null?B.shadowSide:B.side:K.side=B.shadowSide!==null?B.shadowSide:_[B.side],K.alphaMap=B.alphaMap,K.alphaTest=B.alphaToCoverage===!0?.5:B.alphaTest,K.map=B.map,K.clipShadows=B.clipShadows,K.clippingPlanes=B.clippingPlanes,K.clipIntersection=B.clipIntersection,K.displacementMap=B.displacementMap,K.displacementScale=B.displacementScale,K.displacementBias=B.displacementBias,K.wireframeLinewidth=B.wireframeLinewidth,K.linewidth=B.linewidth,R.isPointLight===!0&&K.isMeshDistanceMaterial===!0){const $=r.properties.get(K);$.light=R}return K}function U(O,B,R,z,K){if(O.visible===!1)return;if(O.layers.test(B.layers)&&(O.isMesh||O.isLine||O.isPoints)&&(O.castShadow||O.receiveShadow&&K===pl)&&(!O.frustumCulled||a.intersectsObject(O))){O.modelViewMatrix.multiplyMatrices(R.matrixWorldInverse,O.matrixWorld);const ht=t.update(O),gt=O.material;if(Array.isArray(gt)){const q=ht.groups;for(let P=0,F=q.length;P<F;P++){const ct=q[P],J=gt[ct.materialIndex];if(J&&J.visible){const xt=D(O,J,z,K);O.onBeforeShadow(r,O,B,R,ht,xt,ct),r.renderBufferDirect(R,null,ht,xt,O,ct),O.onAfterShadow(r,O,B,R,ht,xt,ct)}}}else if(gt.visible){const q=D(O,gt,z,K);O.onBeforeShadow(r,O,B,R,ht,q,null),r.renderBufferDirect(R,null,ht,q,O,null),O.onAfterShadow(r,O,B,R,ht,q,null)}}const $=O.children;for(let ht=0,gt=$.length;ht<gt;ht++)U($[ht],B,R,z,K)}function G(O){O.target.removeEventListener("dispose",G);for(const R in h){const z=h[R],K=O.target.uuid;K in z&&(z[K].dispose(),delete z[K])}}}function pR(r,t){function n(){let j=!1;const Ct=new fn;let _t=null;const jt=new fn(0,0,0,0);return{setMask:function(It){_t!==It&&!j&&(r.colorMask(It,It,It,It),_t=It)},setLocked:function(It){j=It},setClear:function(It,Et,Jt,ue,ln){ln===!0&&(It*=ue,Et*=ue,Jt*=ue),Ct.set(It,Et,Jt,ue),jt.equals(Ct)===!1&&(r.clearColor(It,Et,Jt,ue),jt.copy(Ct))},reset:function(){j=!1,_t=null,jt.set(-1,0,0,0)}}}function a(){let j=!1,Ct=!1,_t=null,jt=null,It=null;return{setReversed:function(Et){if(Ct!==Et){const Jt=t.get("EXT_clip_control");Et?Jt.clipControlEXT(Jt.LOWER_LEFT_EXT,Jt.ZERO_TO_ONE_EXT):Jt.clipControlEXT(Jt.LOWER_LEFT_EXT,Jt.NEGATIVE_ONE_TO_ONE_EXT),Ct=Et;const ue=It;It=null,this.setClear(ue)}},getReversed:function(){return Ct},setTest:function(Et){Et?Tt(r.DEPTH_TEST):Wt(r.DEPTH_TEST)},setMask:function(Et){_t!==Et&&!j&&(r.depthMask(Et),_t=Et)},setFunc:function(Et){if(Ct&&(Et=hb[Et]),jt!==Et){switch(Et){case Bh:r.depthFunc(r.NEVER);break;case Fh:r.depthFunc(r.ALWAYS);break;case Hh:r.depthFunc(r.LESS);break;case so:r.depthFunc(r.LEQUAL);break;case Gh:r.depthFunc(r.EQUAL);break;case Vh:r.depthFunc(r.GEQUAL);break;case kh:r.depthFunc(r.GREATER);break;case jh:r.depthFunc(r.NOTEQUAL);break;default:r.depthFunc(r.LEQUAL)}jt=Et}},setLocked:function(Et){j=Et},setClear:function(Et){It!==Et&&(It=Et,Ct&&(Et=1-Et),r.clearDepth(Et))},reset:function(){j=!1,_t=null,jt=null,It=null,Ct=!1}}}function l(){let j=!1,Ct=null,_t=null,jt=null,It=null,Et=null,Jt=null,ue=null,ln=null;return{setTest:function(Ie){j||(Ie?Tt(r.STENCIL_TEST):Wt(r.STENCIL_TEST))},setMask:function(Ie){Ct!==Ie&&!j&&(r.stencilMask(Ie),Ct=Ie)},setFunc:function(Ie,mi,ei){(_t!==Ie||jt!==mi||It!==ei)&&(r.stencilFunc(Ie,mi,ei),_t=Ie,jt=mi,It=ei)},setOp:function(Ie,mi,ei){(Et!==Ie||Jt!==mi||ue!==ei)&&(r.stencilOp(Ie,mi,ei),Et=Ie,Jt=mi,ue=ei)},setLocked:function(Ie){j=Ie},setClear:function(Ie){ln!==Ie&&(r.clearStencil(Ie),ln=Ie)},reset:function(){j=!1,Ct=null,_t=null,jt=null,It=null,Et=null,Jt=null,ue=null,ln=null}}}const c=new n,f=new a,d=new l,m=new WeakMap,h=new WeakMap;let g={},_={},v={},y=new WeakMap,E=[],A=null,S=!1,x=null,w=null,D=null,U=null,G=null,O=null,B=null,R=new _e(0,0,0),z=0,K=!1,V=null,$=null,ht=null,gt=null,q=null;const P=r.getParameter(r.MAX_COMBINED_TEXTURE_IMAGE_UNITS);let F=!1,ct=0;const J=r.getParameter(r.VERSION);J.indexOf("WebGL")!==-1?(ct=parseFloat(/^WebGL (\d)/.exec(J)[1]),F=ct>=1):J.indexOf("OpenGL ES")!==-1&&(ct=parseFloat(/^OpenGL ES (\d)/.exec(J)[1]),F=ct>=2);let xt=null,I={};const Q=r.getParameter(r.SCISSOR_BOX),Mt=r.getParameter(r.VIEWPORT),Rt=new fn().fromArray(Q),wt=new fn().fromArray(Mt);function st(j,Ct,_t,jt){const It=new Uint8Array(4),Et=r.createTexture();r.bindTexture(j,Et),r.texParameteri(j,r.TEXTURE_MIN_FILTER,r.NEAREST),r.texParameteri(j,r.TEXTURE_MAG_FILTER,r.NEAREST);for(let Jt=0;Jt<_t;Jt++)j===r.TEXTURE_3D||j===r.TEXTURE_2D_ARRAY?r.texImage3D(Ct,0,r.RGBA,1,1,jt,0,r.RGBA,r.UNSIGNED_BYTE,It):r.texImage2D(Ct+Jt,0,r.RGBA,1,1,0,r.RGBA,r.UNSIGNED_BYTE,It);return Et}const bt={};bt[r.TEXTURE_2D]=st(r.TEXTURE_2D,r.TEXTURE_2D,1),bt[r.TEXTURE_CUBE_MAP]=st(r.TEXTURE_CUBE_MAP,r.TEXTURE_CUBE_MAP_POSITIVE_X,6),bt[r.TEXTURE_2D_ARRAY]=st(r.TEXTURE_2D_ARRAY,r.TEXTURE_2D_ARRAY,1,1),bt[r.TEXTURE_3D]=st(r.TEXTURE_3D,r.TEXTURE_3D,1,1),c.setClear(0,0,0,1),f.setClear(1),d.setClear(0),Tt(r.DEPTH_TEST),f.setFunc(so),Me(!1),le(Z_),Tt(r.CULL_FACE),Gt(Ra);function Tt(j){g[j]!==!0&&(r.enable(j),g[j]=!0)}function Wt(j){g[j]!==!1&&(r.disable(j),g[j]=!1)}function re(j,Ct){return v[j]!==Ct?(r.bindFramebuffer(j,Ct),v[j]=Ct,j===r.DRAW_FRAMEBUFFER&&(v[r.FRAMEBUFFER]=Ct),j===r.FRAMEBUFFER&&(v[r.DRAW_FRAMEBUFFER]=Ct),!0):!1}function ie(j,Ct){let _t=E,jt=!1;if(j){_t=y.get(Ct),_t===void 0&&(_t=[],y.set(Ct,_t));const It=j.textures;if(_t.length!==It.length||_t[0]!==r.COLOR_ATTACHMENT0){for(let Et=0,Jt=It.length;Et<Jt;Et++)_t[Et]=r.COLOR_ATTACHMENT0+Et;_t.length=It.length,jt=!0}}else _t[0]!==r.BACK&&(_t[0]=r.BACK,jt=!0);jt&&r.drawBuffers(_t)}function Nt(j){return A!==j?(r.useProgram(j),A=j,!0):!1}const Ht={[Xs]:r.FUNC_ADD,[IM]:r.FUNC_SUBTRACT,[zM]:r.FUNC_REVERSE_SUBTRACT};Ht[BM]=r.MIN,Ht[FM]=r.MAX;const Ut={[HM]:r.ZERO,[GM]:r.ONE,[VM]:r.SRC_COLOR,[Ih]:r.SRC_ALPHA,[YM]:r.SRC_ALPHA_SATURATE,[WM]:r.DST_COLOR,[jM]:r.DST_ALPHA,[kM]:r.ONE_MINUS_SRC_COLOR,[zh]:r.ONE_MINUS_SRC_ALPHA,[qM]:r.ONE_MINUS_DST_COLOR,[XM]:r.ONE_MINUS_DST_ALPHA,[ZM]:r.CONSTANT_COLOR,[KM]:r.ONE_MINUS_CONSTANT_COLOR,[QM]:r.CONSTANT_ALPHA,[JM]:r.ONE_MINUS_CONSTANT_ALPHA};function Gt(j,Ct,_t,jt,It,Et,Jt,ue,ln,Ie){if(j===Ra){S===!0&&(Wt(r.BLEND),S=!1);return}if(S===!1&&(Tt(r.BLEND),S=!0),j!==PM){if(j!==x||Ie!==K){if((w!==Xs||G!==Xs)&&(r.blendEquation(r.FUNC_ADD),w=Xs,G=Xs),Ie)switch(j){case io:r.blendFuncSeparate(r.ONE,r.ONE_MINUS_SRC_ALPHA,r.ONE,r.ONE_MINUS_SRC_ALPHA);break;case _l:r.blendFunc(r.ONE,r.ONE);break;case K_:r.blendFuncSeparate(r.ZERO,r.ONE_MINUS_SRC_COLOR,r.ZERO,r.ONE);break;case Q_:r.blendFuncSeparate(r.DST_COLOR,r.ONE_MINUS_SRC_ALPHA,r.ZERO,r.ONE);break;default:Ne("WebGLState: Invalid blending: ",j);break}else switch(j){case io:r.blendFuncSeparate(r.SRC_ALPHA,r.ONE_MINUS_SRC_ALPHA,r.ONE,r.ONE_MINUS_SRC_ALPHA);break;case _l:r.blendFuncSeparate(r.SRC_ALPHA,r.ONE,r.ONE,r.ONE);break;case K_:Ne("WebGLState: SubtractiveBlending requires material.premultipliedAlpha = true");break;case Q_:Ne("WebGLState: MultiplyBlending requires material.premultipliedAlpha = true");break;default:Ne("WebGLState: Invalid blending: ",j);break}D=null,U=null,O=null,B=null,R.set(0,0,0),z=0,x=j,K=Ie}return}It=It||Ct,Et=Et||_t,Jt=Jt||jt,(Ct!==w||It!==G)&&(r.blendEquationSeparate(Ht[Ct],Ht[It]),w=Ct,G=It),(_t!==D||jt!==U||Et!==O||Jt!==B)&&(r.blendFuncSeparate(Ut[_t],Ut[jt],Ut[Et],Ut[Jt]),D=_t,U=jt,O=Et,B=Jt),(ue.equals(R)===!1||ln!==z)&&(r.blendColor(ue.r,ue.g,ue.b,ln),R.copy(ue),z=ln),x=j,K=!1}function ne(j,Ct){j.side===Aa?Wt(r.CULL_FACE):Tt(r.CULL_FACE);let _t=j.side===ti;Ct&&(_t=!_t),Me(_t),j.blending===io&&j.transparent===!1?Gt(Ra):Gt(j.blending,j.blendEquation,j.blendSrc,j.blendDst,j.blendEquationAlpha,j.blendSrcAlpha,j.blendDstAlpha,j.blendColor,j.blendAlpha,j.premultipliedAlpha),f.setFunc(j.depthFunc),f.setTest(j.depthTest),f.setMask(j.depthWrite),c.setMask(j.colorWrite);const jt=j.stencilWrite;d.setTest(jt),jt&&(d.setMask(j.stencilWriteMask),d.setFunc(j.stencilFunc,j.stencilRef,j.stencilFuncMask),d.setOp(j.stencilFail,j.stencilZFail,j.stencilZPass)),W(j.polygonOffset,j.polygonOffsetFactor,j.polygonOffsetUnits),j.alphaToCoverage===!0?Tt(r.SAMPLE_ALPHA_TO_COVERAGE):Wt(r.SAMPLE_ALPHA_TO_COVERAGE)}function Me(j){V!==j&&(j?r.frontFace(r.CW):r.frontFace(r.CCW),V=j)}function le(j){j!==UM?(Tt(r.CULL_FACE),j!==$&&(j===Z_?r.cullFace(r.BACK):j===LM?r.cullFace(r.FRONT):r.cullFace(r.FRONT_AND_BACK))):Wt(r.CULL_FACE),$=j}function Ue(j){j!==ht&&(F&&r.lineWidth(j),ht=j)}function W(j,Ct,_t){j?(Tt(r.POLYGON_OFFSET_FILL),(gt!==Ct||q!==_t)&&(gt=Ct,q=_t,f.getReversed()&&(Ct=-Ct),r.polygonOffset(Ct,_t))):Wt(r.POLYGON_OFFSET_FILL)}function We(j){j?Tt(r.SCISSOR_TEST):Wt(r.SCISSOR_TEST)}function ye(j){j===void 0&&(j=r.TEXTURE0+P-1),xt!==j&&(r.activeTexture(j),xt=j)}function qe(j,Ct,_t){_t===void 0&&(xt===null?_t=r.TEXTURE0+P-1:_t=xt);let jt=I[_t];jt===void 0&&(jt={type:void 0,texture:void 0},I[_t]=jt),(jt.type!==j||jt.texture!==Ct)&&(xt!==_t&&(r.activeTexture(_t),xt=_t),r.bindTexture(j,Ct||bt[j]),jt.type=j,jt.texture=Ct)}function Dt(){const j=I[xt];j!==void 0&&j.type!==void 0&&(r.bindTexture(j.type,null),j.type=void 0,j.texture=void 0)}function an(){try{r.compressedTexImage2D(...arguments)}catch(j){Ne("WebGLState:",j)}}function L(){try{r.compressedTexImage3D(...arguments)}catch(j){Ne("WebGLState:",j)}}function T(){try{r.texSubImage2D(...arguments)}catch(j){Ne("WebGLState:",j)}}function tt(){try{r.texSubImage3D(...arguments)}catch(j){Ne("WebGLState:",j)}}function yt(){try{r.compressedTexSubImage2D(...arguments)}catch(j){Ne("WebGLState:",j)}}function At(){try{r.compressedTexSubImage3D(...arguments)}catch(j){Ne("WebGLState:",j)}}function Lt(){try{r.texStorage2D(...arguments)}catch(j){Ne("WebGLState:",j)}}function zt(){try{r.texStorage3D(...arguments)}catch(j){Ne("WebGLState:",j)}}function dt(){try{r.texImage2D(...arguments)}catch(j){Ne("WebGLState:",j)}}function pt(){try{r.texImage3D(...arguments)}catch(j){Ne("WebGLState:",j)}}function Bt(j){return _[j]!==void 0?_[j]:r.getParameter(j)}function Ft(j,Ct){_[j]!==Ct&&(r.pixelStorei(j,Ct),_[j]=Ct)}function Pt(j){Rt.equals(j)===!1&&(r.scissor(j.x,j.y,j.z,j.w),Rt.copy(j))}function Ot(j){wt.equals(j)===!1&&(r.viewport(j.x,j.y,j.z,j.w),wt.copy(j))}function fe(j,Ct){let _t=h.get(Ct);_t===void 0&&(_t=new WeakMap,h.set(Ct,_t));let jt=_t.get(j);jt===void 0&&(jt=r.getUniformBlockIndex(Ct,j.name),_t.set(j,jt))}function de(j,Ct){const jt=h.get(Ct).get(j);m.get(Ct)!==jt&&(r.uniformBlockBinding(Ct,jt,j.__bindingPointIndex),m.set(Ct,jt))}function be(){r.disable(r.BLEND),r.disable(r.CULL_FACE),r.disable(r.DEPTH_TEST),r.disable(r.POLYGON_OFFSET_FILL),r.disable(r.SCISSOR_TEST),r.disable(r.STENCIL_TEST),r.disable(r.SAMPLE_ALPHA_TO_COVERAGE),r.blendEquation(r.FUNC_ADD),r.blendFunc(r.ONE,r.ZERO),r.blendFuncSeparate(r.ONE,r.ZERO,r.ONE,r.ZERO),r.blendColor(0,0,0,0),r.colorMask(!0,!0,!0,!0),r.clearColor(0,0,0,0),r.depthMask(!0),r.depthFunc(r.LESS),f.setReversed(!1),r.clearDepth(1),r.stencilMask(4294967295),r.stencilFunc(r.ALWAYS,0,4294967295),r.stencilOp(r.KEEP,r.KEEP,r.KEEP),r.clearStencil(0),r.cullFace(r.BACK),r.frontFace(r.CCW),r.polygonOffset(0,0),r.activeTexture(r.TEXTURE0),r.bindFramebuffer(r.FRAMEBUFFER,null),r.bindFramebuffer(r.DRAW_FRAMEBUFFER,null),r.bindFramebuffer(r.READ_FRAMEBUFFER,null),r.useProgram(null),r.lineWidth(1),r.scissor(0,0,r.canvas.width,r.canvas.height),r.viewport(0,0,r.canvas.width,r.canvas.height),r.pixelStorei(r.PACK_ALIGNMENT,4),r.pixelStorei(r.UNPACK_ALIGNMENT,4),r.pixelStorei(r.UNPACK_FLIP_Y_WEBGL,!1),r.pixelStorei(r.UNPACK_PREMULTIPLY_ALPHA_WEBGL,!1),r.pixelStorei(r.UNPACK_COLORSPACE_CONVERSION_WEBGL,r.BROWSER_DEFAULT_WEBGL),r.pixelStorei(r.PACK_ROW_LENGTH,0),r.pixelStorei(r.PACK_SKIP_PIXELS,0),r.pixelStorei(r.PACK_SKIP_ROWS,0),r.pixelStorei(r.UNPACK_ROW_LENGTH,0),r.pixelStorei(r.UNPACK_IMAGE_HEIGHT,0),r.pixelStorei(r.UNPACK_SKIP_PIXELS,0),r.pixelStorei(r.UNPACK_SKIP_ROWS,0),r.pixelStorei(r.UNPACK_SKIP_IMAGES,0),g={},_={},xt=null,I={},v={},y=new WeakMap,E=[],A=null,S=!1,x=null,w=null,D=null,U=null,G=null,O=null,B=null,R=new _e(0,0,0),z=0,K=!1,V=null,$=null,ht=null,gt=null,q=null,Rt.set(0,0,r.canvas.width,r.canvas.height),wt.set(0,0,r.canvas.width,r.canvas.height),c.reset(),f.reset(),d.reset()}return{buffers:{color:c,depth:f,stencil:d},enable:Tt,disable:Wt,bindFramebuffer:re,drawBuffers:ie,useProgram:Nt,setBlending:Gt,setMaterial:ne,setFlipSided:Me,setCullFace:le,setLineWidth:Ue,setPolygonOffset:W,setScissorTest:We,activeTexture:ye,bindTexture:qe,unbindTexture:Dt,compressedTexImage2D:an,compressedTexImage3D:L,texImage2D:dt,texImage3D:pt,pixelStorei:Ft,getParameter:Bt,updateUBOMapping:fe,uniformBlockBinding:de,texStorage2D:Lt,texStorage3D:zt,texSubImage2D:T,texSubImage3D:tt,compressedTexSubImage2D:yt,compressedTexSubImage3D:At,scissor:Pt,viewport:Ot,reset:be}}function mR(r,t,n,a,l,c,f){const d=t.has("WEBGL_multisampled_render_to_texture")?t.get("WEBGL_multisampled_render_to_texture"):null,m=typeof navigator>"u"?!1:/OculusBrowser/g.test(navigator.userAgent),h=new ee,g=new WeakMap,_=new Set;let v;const y=new WeakMap;let E=!1;try{E=typeof OffscreenCanvas<"u"&&new OffscreenCanvas(1,1).getContext("2d")!==null}catch{}function A(L,T){return E?new OffscreenCanvas(L,T):Au("canvas")}function S(L,T,tt){let yt=1;const At=an(L);if((At.width>tt||At.height>tt)&&(yt=tt/Math.max(At.width,At.height)),yt<1)if(typeof HTMLImageElement<"u"&&L instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&L instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&L instanceof ImageBitmap||typeof VideoFrame<"u"&&L instanceof VideoFrame){const Lt=Math.floor(yt*At.width),zt=Math.floor(yt*At.height);v===void 0&&(v=A(Lt,zt));const dt=T?A(Lt,zt):v;return dt.width=Lt,dt.height=zt,dt.getContext("2d").drawImage(L,0,0,Lt,zt),ce("WebGLRenderer: Texture has been resized from ("+At.width+"x"+At.height+") to ("+Lt+"x"+zt+")."),dt}else return"data"in L&&ce("WebGLRenderer: Image in DataTexture is too big ("+At.width+"x"+At.height+")."),L;return L}function x(L){return L.generateMipmaps}function w(L){r.generateMipmap(L)}function D(L){return L.isWebGLCubeRenderTarget?r.TEXTURE_CUBE_MAP:L.isWebGL3DRenderTarget?r.TEXTURE_3D:L.isWebGLArrayRenderTarget||L.isCompressedArrayTexture?r.TEXTURE_2D_ARRAY:r.TEXTURE_2D}function U(L,T,tt,yt,At,Lt=!1){if(L!==null){if(r[L]!==void 0)return r[L];ce("WebGLRenderer: Attempt to use non-existing WebGL internal format '"+L+"'")}let zt;yt&&(zt=t.get("EXT_texture_norm16"),zt||ce("WebGLRenderer: Unable to use normalized textures without EXT_texture_norm16 extension"));let dt=T;if(T===r.RED&&(tt===r.FLOAT&&(dt=r.R32F),tt===r.HALF_FLOAT&&(dt=r.R16F),tt===r.UNSIGNED_BYTE&&(dt=r.R8),tt===r.UNSIGNED_SHORT&&zt&&(dt=zt.R16_EXT),tt===r.SHORT&&zt&&(dt=zt.R16_SNORM_EXT)),T===r.RED_INTEGER&&(tt===r.UNSIGNED_BYTE&&(dt=r.R8UI),tt===r.UNSIGNED_SHORT&&(dt=r.R16UI),tt===r.UNSIGNED_INT&&(dt=r.R32UI),tt===r.BYTE&&(dt=r.R8I),tt===r.SHORT&&(dt=r.R16I),tt===r.INT&&(dt=r.R32I)),T===r.RG&&(tt===r.FLOAT&&(dt=r.RG32F),tt===r.HALF_FLOAT&&(dt=r.RG16F),tt===r.UNSIGNED_BYTE&&(dt=r.RG8),tt===r.UNSIGNED_SHORT&&zt&&(dt=zt.RG16_EXT),tt===r.SHORT&&zt&&(dt=zt.RG16_SNORM_EXT)),T===r.RG_INTEGER&&(tt===r.UNSIGNED_BYTE&&(dt=r.RG8UI),tt===r.UNSIGNED_SHORT&&(dt=r.RG16UI),tt===r.UNSIGNED_INT&&(dt=r.RG32UI),tt===r.BYTE&&(dt=r.RG8I),tt===r.SHORT&&(dt=r.RG16I),tt===r.INT&&(dt=r.RG32I)),T===r.RGB_INTEGER&&(tt===r.UNSIGNED_BYTE&&(dt=r.RGB8UI),tt===r.UNSIGNED_SHORT&&(dt=r.RGB16UI),tt===r.UNSIGNED_INT&&(dt=r.RGB32UI),tt===r.BYTE&&(dt=r.RGB8I),tt===r.SHORT&&(dt=r.RGB16I),tt===r.INT&&(dt=r.RGB32I)),T===r.RGBA_INTEGER&&(tt===r.UNSIGNED_BYTE&&(dt=r.RGBA8UI),tt===r.UNSIGNED_SHORT&&(dt=r.RGBA16UI),tt===r.UNSIGNED_INT&&(dt=r.RGBA32UI),tt===r.BYTE&&(dt=r.RGBA8I),tt===r.SHORT&&(dt=r.RGBA16I),tt===r.INT&&(dt=r.RGBA32I)),T===r.RGB&&(tt===r.UNSIGNED_SHORT&&zt&&(dt=zt.RGB16_EXT),tt===r.SHORT&&zt&&(dt=zt.RGB16_SNORM_EXT),tt===r.UNSIGNED_INT_5_9_9_9_REV&&(dt=r.RGB9_E5),tt===r.UNSIGNED_INT_10F_11F_11F_REV&&(dt=r.R11F_G11F_B10F)),T===r.RGBA){const pt=Lt?Tu:De.getTransfer(At);tt===r.FLOAT&&(dt=r.RGBA32F),tt===r.HALF_FLOAT&&(dt=r.RGBA16F),tt===r.UNSIGNED_BYTE&&(dt=pt===je?r.SRGB8_ALPHA8:r.RGBA8),tt===r.UNSIGNED_SHORT&&zt&&(dt=zt.RGBA16_EXT),tt===r.SHORT&&zt&&(dt=zt.RGBA16_SNORM_EXT),tt===r.UNSIGNED_SHORT_4_4_4_4&&(dt=r.RGBA4),tt===r.UNSIGNED_SHORT_5_5_5_1&&(dt=r.RGB5_A1)}return(dt===r.R16F||dt===r.R32F||dt===r.RG16F||dt===r.RG32F||dt===r.RGBA16F||dt===r.RGBA32F)&&t.get("EXT_color_buffer_float"),dt}function G(L,T){let tt;return L?T===null||T===ea||T===bl?tt=r.DEPTH24_STENCIL8:T===Qi?tt=r.DEPTH32F_STENCIL8:T===Ml&&(tt=r.DEPTH24_STENCIL8,ce("DepthTexture: 16 bit depth attachment is not supported with stencil. Using 24-bit attachment.")):T===null||T===ea||T===bl?tt=r.DEPTH_COMPONENT24:T===Qi?tt=r.DEPTH_COMPONENT32F:T===Ml&&(tt=r.DEPTH_COMPONENT16),tt}function O(L,T){return x(L)===!0||L.isFramebufferTexture&&L.minFilter!==Pn&&L.minFilter!==Sn?Math.log2(Math.max(T.width,T.height))+1:L.mipmaps!==void 0&&L.mipmaps.length>0?L.mipmaps.length:L.isCompressedTexture&&Array.isArray(L.image)?T.mipmaps.length:1}function B(L){const T=L.target;T.removeEventListener("dispose",B),z(T),T.isVideoTexture&&g.delete(T),T.isHTMLTexture&&_.delete(T)}function R(L){const T=L.target;T.removeEventListener("dispose",R),V(T)}function z(L){const T=a.get(L);if(T.__webglInit===void 0)return;const tt=L.source,yt=y.get(tt);if(yt){const At=yt[T.__cacheKey];At.usedTimes--,At.usedTimes===0&&K(L),Object.keys(yt).length===0&&y.delete(tt)}a.remove(L)}function K(L){const T=a.get(L);r.deleteTexture(T.__webglTexture);const tt=L.source,yt=y.get(tt);delete yt[T.__cacheKey],f.memory.textures--}function V(L){const T=a.get(L);if(L.depthTexture&&(L.depthTexture.dispose(),a.remove(L.depthTexture)),L.isWebGLCubeRenderTarget)for(let yt=0;yt<6;yt++){if(Array.isArray(T.__webglFramebuffer[yt]))for(let At=0;At<T.__webglFramebuffer[yt].length;At++)r.deleteFramebuffer(T.__webglFramebuffer[yt][At]);else r.deleteFramebuffer(T.__webglFramebuffer[yt]);T.__webglDepthbuffer&&r.deleteRenderbuffer(T.__webglDepthbuffer[yt])}else{if(Array.isArray(T.__webglFramebuffer))for(let yt=0;yt<T.__webglFramebuffer.length;yt++)r.deleteFramebuffer(T.__webglFramebuffer[yt]);else r.deleteFramebuffer(T.__webglFramebuffer);if(T.__webglDepthbuffer&&r.deleteRenderbuffer(T.__webglDepthbuffer),T.__webglMultisampledFramebuffer&&r.deleteFramebuffer(T.__webglMultisampledFramebuffer),T.__webglColorRenderbuffer)for(let yt=0;yt<T.__webglColorRenderbuffer.length;yt++)T.__webglColorRenderbuffer[yt]&&r.deleteRenderbuffer(T.__webglColorRenderbuffer[yt]);T.__webglDepthRenderbuffer&&r.deleteRenderbuffer(T.__webglDepthRenderbuffer)}const tt=L.textures;for(let yt=0,At=tt.length;yt<At;yt++){const Lt=a.get(tt[yt]);Lt.__webglTexture&&(r.deleteTexture(Lt.__webglTexture),f.memory.textures--),a.remove(tt[yt])}a.remove(L)}let $=0;function ht(){$=0}function gt(){return $}function q(L){$=L}function P(){const L=$;return L>=l.maxTextures&&ce("WebGLTextures: Trying to use "+L+" texture units while this GPU supports only "+l.maxTextures),$+=1,L}function F(L){const T=[];return T.push(L.wrapS),T.push(L.wrapT),T.push(L.wrapR||0),T.push(L.magFilter),T.push(L.minFilter),T.push(L.anisotropy),T.push(L.internalFormat),T.push(L.format),T.push(L.type),T.push(L.generateMipmaps),T.push(L.premultiplyAlpha),T.push(L.flipY),T.push(L.unpackAlignment),T.push(L.colorSpace),T.join()}function ct(L,T){const tt=a.get(L);if(L.isVideoTexture&&qe(L),L.isRenderTargetTexture===!1&&L.isExternalTexture!==!0&&L.version>0&&tt.__version!==L.version){const yt=L.image;if(yt===null)ce("WebGLRenderer: Texture marked for update but no image data found.");else if(yt.complete===!1)ce("WebGLRenderer: Texture marked for update but image is incomplete");else{Wt(tt,L,T);return}}else L.isExternalTexture&&(tt.__webglTexture=L.sourceTexture?L.sourceTexture:null);n.bindTexture(r.TEXTURE_2D,tt.__webglTexture,r.TEXTURE0+T)}function J(L,T){const tt=a.get(L);if(L.isRenderTargetTexture===!1&&L.version>0&&tt.__version!==L.version){Wt(tt,L,T);return}else L.isExternalTexture&&(tt.__webglTexture=L.sourceTexture?L.sourceTexture:null);n.bindTexture(r.TEXTURE_2D_ARRAY,tt.__webglTexture,r.TEXTURE0+T)}function xt(L,T){const tt=a.get(L);if(L.isRenderTargetTexture===!1&&L.version>0&&tt.__version!==L.version){Wt(tt,L,T);return}n.bindTexture(r.TEXTURE_3D,tt.__webglTexture,r.TEXTURE0+T)}function I(L,T){const tt=a.get(L);if(L.isCubeDepthTexture!==!0&&L.version>0&&tt.__version!==L.version){re(tt,L,T);return}n.bindTexture(r.TEXTURE_CUBE_MAP,tt.__webglTexture,r.TEXTURE0+T)}const Q={[Xh]:r.REPEAT,[wa]:r.CLAMP_TO_EDGE,[Wh]:r.MIRRORED_REPEAT},Mt={[Pn]:r.NEAREST,[eb]:r.NEAREST_MIPMAP_NEAREST,[Fc]:r.NEAREST_MIPMAP_LINEAR,[Sn]:r.LINEAR,[$d]:r.LINEAR_MIPMAP_NEAREST,[Zs]:r.LINEAR_MIPMAP_LINEAR},Rt={[ab]:r.NEVER,[cb]:r.ALWAYS,[sb]:r.LESS,[kp]:r.LEQUAL,[rb]:r.EQUAL,[jp]:r.GEQUAL,[ob]:r.GREATER,[lb]:r.NOTEQUAL};function wt(L,T){if(T.type===Qi&&t.has("OES_texture_float_linear")===!1&&(T.magFilter===Sn||T.magFilter===$d||T.magFilter===Fc||T.magFilter===Zs||T.minFilter===Sn||T.minFilter===$d||T.minFilter===Fc||T.minFilter===Zs)&&ce("WebGLRenderer: Unable to use linear filtering with floating point textures. OES_texture_float_linear not supported on this device."),r.texParameteri(L,r.TEXTURE_WRAP_S,Q[T.wrapS]),r.texParameteri(L,r.TEXTURE_WRAP_T,Q[T.wrapT]),(L===r.TEXTURE_3D||L===r.TEXTURE_2D_ARRAY)&&r.texParameteri(L,r.TEXTURE_WRAP_R,Q[T.wrapR]),r.texParameteri(L,r.TEXTURE_MAG_FILTER,Mt[T.magFilter]),r.texParameteri(L,r.TEXTURE_MIN_FILTER,Mt[T.minFilter]),T.compareFunction&&(r.texParameteri(L,r.TEXTURE_COMPARE_MODE,r.COMPARE_REF_TO_TEXTURE),r.texParameteri(L,r.TEXTURE_COMPARE_FUNC,Rt[T.compareFunction])),t.has("EXT_texture_filter_anisotropic")===!0){if(T.magFilter===Pn||T.minFilter!==Fc&&T.minFilter!==Zs||T.type===Qi&&t.has("OES_texture_float_linear")===!1)return;if(T.anisotropy>1||a.get(T).__currentAnisotropy){const tt=t.get("EXT_texture_filter_anisotropic");r.texParameterf(L,tt.TEXTURE_MAX_ANISOTROPY_EXT,Math.min(T.anisotropy,l.getMaxAnisotropy())),a.get(T).__currentAnisotropy=T.anisotropy}}}function st(L,T){let tt=!1;L.__webglInit===void 0&&(L.__webglInit=!0,T.addEventListener("dispose",B));const yt=T.source;let At=y.get(yt);At===void 0&&(At={},y.set(yt,At));const Lt=F(T);if(Lt!==L.__cacheKey){At[Lt]===void 0&&(At[Lt]={texture:r.createTexture(),usedTimes:0},f.memory.textures++,tt=!0),At[Lt].usedTimes++;const zt=At[L.__cacheKey];zt!==void 0&&(At[L.__cacheKey].usedTimes--,zt.usedTimes===0&&K(T)),L.__cacheKey=Lt,L.__webglTexture=At[Lt].texture}return tt}function bt(L,T,tt){return Math.floor(Math.floor(L/tt)/T)}function Tt(L,T,tt,yt){const Lt=L.updateRanges;if(Lt.length===0)n.texSubImage2D(r.TEXTURE_2D,0,0,0,T.width,T.height,tt,yt,T.data);else{Lt.sort((Ft,Pt)=>Ft.start-Pt.start);let zt=0;for(let Ft=1;Ft<Lt.length;Ft++){const Pt=Lt[zt],Ot=Lt[Ft],fe=Pt.start+Pt.count,de=bt(Ot.start,T.width,4),be=bt(Pt.start,T.width,4);Ot.start<=fe+1&&de===be&&bt(Ot.start+Ot.count-1,T.width,4)===de?Pt.count=Math.max(Pt.count,Ot.start+Ot.count-Pt.start):(++zt,Lt[zt]=Ot)}Lt.length=zt+1;const dt=n.getParameter(r.UNPACK_ROW_LENGTH),pt=n.getParameter(r.UNPACK_SKIP_PIXELS),Bt=n.getParameter(r.UNPACK_SKIP_ROWS);n.pixelStorei(r.UNPACK_ROW_LENGTH,T.width);for(let Ft=0,Pt=Lt.length;Ft<Pt;Ft++){const Ot=Lt[Ft],fe=Math.floor(Ot.start/4),de=Math.ceil(Ot.count/4),be=fe%T.width,j=Math.floor(fe/T.width),Ct=de,_t=1;n.pixelStorei(r.UNPACK_SKIP_PIXELS,be),n.pixelStorei(r.UNPACK_SKIP_ROWS,j),n.texSubImage2D(r.TEXTURE_2D,0,be,j,Ct,_t,tt,yt,T.data)}L.clearUpdateRanges(),n.pixelStorei(r.UNPACK_ROW_LENGTH,dt),n.pixelStorei(r.UNPACK_SKIP_PIXELS,pt),n.pixelStorei(r.UNPACK_SKIP_ROWS,Bt)}}function Wt(L,T,tt){let yt=r.TEXTURE_2D;(T.isDataArrayTexture||T.isCompressedArrayTexture)&&(yt=r.TEXTURE_2D_ARRAY),T.isData3DTexture&&(yt=r.TEXTURE_3D);const At=st(L,T),Lt=T.source;n.bindTexture(yt,L.__webglTexture,r.TEXTURE0+tt);const zt=a.get(Lt);if(Lt.version!==zt.__version||At===!0){if(n.activeTexture(r.TEXTURE0+tt),(typeof ImageBitmap<"u"&&T.image instanceof ImageBitmap)===!1){const _t=De.getPrimaries(De.workingColorSpace),jt=T.colorSpace===gs?null:De.getPrimaries(T.colorSpace),It=T.colorSpace===gs||_t===jt?r.NONE:r.BROWSER_DEFAULT_WEBGL;n.pixelStorei(r.UNPACK_FLIP_Y_WEBGL,T.flipY),n.pixelStorei(r.UNPACK_PREMULTIPLY_ALPHA_WEBGL,T.premultiplyAlpha),n.pixelStorei(r.UNPACK_COLORSPACE_CONVERSION_WEBGL,It)}n.pixelStorei(r.UNPACK_ALIGNMENT,T.unpackAlignment);let pt=S(T.image,!1,l.maxTextureSize);pt=Dt(T,pt);const Bt=c.convert(T.format,T.colorSpace),Ft=c.convert(T.type);let Pt=U(T.internalFormat,Bt,Ft,T.normalized,T.colorSpace,T.isVideoTexture);wt(yt,T);let Ot;const fe=T.mipmaps,de=T.isVideoTexture!==!0,be=zt.__version===void 0||At===!0,j=Lt.dataReady,Ct=O(T,pt);if(T.isDepthTexture)Pt=G(T.format===Ks,T.type),be&&(de?n.texStorage2D(r.TEXTURE_2D,1,Pt,pt.width,pt.height):n.texImage2D(r.TEXTURE_2D,0,Pt,pt.width,pt.height,0,Bt,Ft,null));else if(T.isDataTexture)if(fe.length>0){de&&be&&n.texStorage2D(r.TEXTURE_2D,Ct,Pt,fe[0].width,fe[0].height);for(let _t=0,jt=fe.length;_t<jt;_t++)Ot=fe[_t],de?j&&n.texSubImage2D(r.TEXTURE_2D,_t,0,0,Ot.width,Ot.height,Bt,Ft,Ot.data):n.texImage2D(r.TEXTURE_2D,_t,Pt,Ot.width,Ot.height,0,Bt,Ft,Ot.data);T.generateMipmaps=!1}else de?(be&&n.texStorage2D(r.TEXTURE_2D,Ct,Pt,pt.width,pt.height),j&&Tt(T,pt,Bt,Ft)):n.texImage2D(r.TEXTURE_2D,0,Pt,pt.width,pt.height,0,Bt,Ft,pt.data);else if(T.isCompressedTexture)if(T.isCompressedArrayTexture){de&&be&&n.texStorage3D(r.TEXTURE_2D_ARRAY,Ct,Pt,fe[0].width,fe[0].height,pt.depth);for(let _t=0,jt=fe.length;_t<jt;_t++)if(Ot=fe[_t],T.format!==Hi)if(Bt!==null)if(de){if(j)if(T.layerUpdates.size>0){const It=Dv(Ot.width,Ot.height,T.format,T.type);for(const Et of T.layerUpdates){const Jt=Ot.data.subarray(Et*It/Ot.data.BYTES_PER_ELEMENT,(Et+1)*It/Ot.data.BYTES_PER_ELEMENT);n.compressedTexSubImage3D(r.TEXTURE_2D_ARRAY,_t,0,0,Et,Ot.width,Ot.height,1,Bt,Jt)}T.clearLayerUpdates()}else n.compressedTexSubImage3D(r.TEXTURE_2D_ARRAY,_t,0,0,0,Ot.width,Ot.height,pt.depth,Bt,Ot.data)}else n.compressedTexImage3D(r.TEXTURE_2D_ARRAY,_t,Pt,Ot.width,Ot.height,pt.depth,0,Ot.data,0,0);else ce("WebGLRenderer: Attempt to load unsupported compressed texture format in .uploadTexture()");else de?j&&n.texSubImage3D(r.TEXTURE_2D_ARRAY,_t,0,0,0,Ot.width,Ot.height,pt.depth,Bt,Ft,Ot.data):n.texImage3D(r.TEXTURE_2D_ARRAY,_t,Pt,Ot.width,Ot.height,pt.depth,0,Bt,Ft,Ot.data)}else{de&&be&&n.texStorage2D(r.TEXTURE_2D,Ct,Pt,fe[0].width,fe[0].height);for(let _t=0,jt=fe.length;_t<jt;_t++)Ot=fe[_t],T.format!==Hi?Bt!==null?de?j&&n.compressedTexSubImage2D(r.TEXTURE_2D,_t,0,0,Ot.width,Ot.height,Bt,Ot.data):n.compressedTexImage2D(r.TEXTURE_2D,_t,Pt,Ot.width,Ot.height,0,Ot.data):ce("WebGLRenderer: Attempt to load unsupported compressed texture format in .uploadTexture()"):de?j&&n.texSubImage2D(r.TEXTURE_2D,_t,0,0,Ot.width,Ot.height,Bt,Ft,Ot.data):n.texImage2D(r.TEXTURE_2D,_t,Pt,Ot.width,Ot.height,0,Bt,Ft,Ot.data)}else if(T.isDataArrayTexture)if(de){if(be&&n.texStorage3D(r.TEXTURE_2D_ARRAY,Ct,Pt,pt.width,pt.height,pt.depth),j)if(T.layerUpdates.size>0){const _t=Dv(pt.width,pt.height,T.format,T.type);for(const jt of T.layerUpdates){const It=pt.data.subarray(jt*_t/pt.data.BYTES_PER_ELEMENT,(jt+1)*_t/pt.data.BYTES_PER_ELEMENT);n.texSubImage3D(r.TEXTURE_2D_ARRAY,0,0,0,jt,pt.width,pt.height,1,Bt,Ft,It)}T.clearLayerUpdates()}else n.texSubImage3D(r.TEXTURE_2D_ARRAY,0,0,0,0,pt.width,pt.height,pt.depth,Bt,Ft,pt.data)}else n.texImage3D(r.TEXTURE_2D_ARRAY,0,Pt,pt.width,pt.height,pt.depth,0,Bt,Ft,pt.data);else if(T.isData3DTexture)de?(be&&n.texStorage3D(r.TEXTURE_3D,Ct,Pt,pt.width,pt.height,pt.depth),j&&n.texSubImage3D(r.TEXTURE_3D,0,0,0,0,pt.width,pt.height,pt.depth,Bt,Ft,pt.data)):n.texImage3D(r.TEXTURE_3D,0,Pt,pt.width,pt.height,pt.depth,0,Bt,Ft,pt.data);else if(T.isFramebufferTexture){if(be)if(de)n.texStorage2D(r.TEXTURE_2D,Ct,Pt,pt.width,pt.height);else{let _t=pt.width,jt=pt.height;for(let It=0;It<Ct;It++)n.texImage2D(r.TEXTURE_2D,It,Pt,_t,jt,0,Bt,Ft,null),_t>>=1,jt>>=1}}else if(T.isHTMLTexture){if("texElementImage2D"in r){const _t=r.canvas;if(_t.hasAttribute("layoutsubtree")||_t.setAttribute("layoutsubtree","true"),pt.parentNode!==_t){_t.appendChild(pt),_.add(T),_t.onpaint=ue=>{const ln=ue.changedElements;for(const Ie of _)ln.includes(Ie.image)&&(Ie.needsUpdate=!0)},_t.requestPaint();return}const jt=0,It=r.RGBA,Et=r.RGBA,Jt=r.UNSIGNED_BYTE;r.texElementImage2D(r.TEXTURE_2D,jt,It,Et,Jt,pt),r.texParameteri(r.TEXTURE_2D,r.TEXTURE_MIN_FILTER,r.LINEAR),r.texParameteri(r.TEXTURE_2D,r.TEXTURE_WRAP_S,r.CLAMP_TO_EDGE),r.texParameteri(r.TEXTURE_2D,r.TEXTURE_WRAP_T,r.CLAMP_TO_EDGE)}}else if(fe.length>0){if(de&&be){const _t=an(fe[0]);n.texStorage2D(r.TEXTURE_2D,Ct,Pt,_t.width,_t.height)}for(let _t=0,jt=fe.length;_t<jt;_t++)Ot=fe[_t],de?j&&n.texSubImage2D(r.TEXTURE_2D,_t,0,0,Bt,Ft,Ot):n.texImage2D(r.TEXTURE_2D,_t,Pt,Bt,Ft,Ot);T.generateMipmaps=!1}else if(de){if(be){const _t=an(pt);n.texStorage2D(r.TEXTURE_2D,Ct,Pt,_t.width,_t.height)}j&&n.texSubImage2D(r.TEXTURE_2D,0,0,0,Bt,Ft,pt)}else n.texImage2D(r.TEXTURE_2D,0,Pt,Bt,Ft,pt);x(T)&&w(yt),zt.__version=Lt.version,T.onUpdate&&T.onUpdate(T)}L.__version=T.version}function re(L,T,tt){if(T.image.length!==6)return;const yt=st(L,T),At=T.source;n.bindTexture(r.TEXTURE_CUBE_MAP,L.__webglTexture,r.TEXTURE0+tt);const Lt=a.get(At);if(At.version!==Lt.__version||yt===!0){n.activeTexture(r.TEXTURE0+tt);const zt=De.getPrimaries(De.workingColorSpace),dt=T.colorSpace===gs?null:De.getPrimaries(T.colorSpace),pt=T.colorSpace===gs||zt===dt?r.NONE:r.BROWSER_DEFAULT_WEBGL;n.pixelStorei(r.UNPACK_FLIP_Y_WEBGL,T.flipY),n.pixelStorei(r.UNPACK_PREMULTIPLY_ALPHA_WEBGL,T.premultiplyAlpha),n.pixelStorei(r.UNPACK_ALIGNMENT,T.unpackAlignment),n.pixelStorei(r.UNPACK_COLORSPACE_CONVERSION_WEBGL,pt);const Bt=T.isCompressedTexture||T.image[0].isCompressedTexture,Ft=T.image[0]&&T.image[0].isDataTexture,Pt=[];for(let Et=0;Et<6;Et++)!Bt&&!Ft?Pt[Et]=S(T.image[Et],!0,l.maxCubemapSize):Pt[Et]=Ft?T.image[Et].image:T.image[Et],Pt[Et]=Dt(T,Pt[Et]);const Ot=Pt[0],fe=c.convert(T.format,T.colorSpace),de=c.convert(T.type),be=U(T.internalFormat,fe,de,T.normalized,T.colorSpace),j=T.isVideoTexture!==!0,Ct=Lt.__version===void 0||yt===!0,_t=At.dataReady;let jt=O(T,Ot);wt(r.TEXTURE_CUBE_MAP,T);let It;if(Bt){j&&Ct&&n.texStorage2D(r.TEXTURE_CUBE_MAP,jt,be,Ot.width,Ot.height);for(let Et=0;Et<6;Et++){It=Pt[Et].mipmaps;for(let Jt=0;Jt<It.length;Jt++){const ue=It[Jt];T.format!==Hi?fe!==null?j?_t&&n.compressedTexSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt,0,0,ue.width,ue.height,fe,ue.data):n.compressedTexImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt,be,ue.width,ue.height,0,ue.data):ce("WebGLRenderer: Attempt to load unsupported compressed texture format in .setTextureCube()"):j?_t&&n.texSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt,0,0,ue.width,ue.height,fe,de,ue.data):n.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt,be,ue.width,ue.height,0,fe,de,ue.data)}}}else{if(It=T.mipmaps,j&&Ct){It.length>0&&jt++;const Et=an(Pt[0]);n.texStorage2D(r.TEXTURE_CUBE_MAP,jt,be,Et.width,Et.height)}for(let Et=0;Et<6;Et++)if(Ft){j?_t&&n.texSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,0,0,0,Pt[Et].width,Pt[Et].height,fe,de,Pt[Et].data):n.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,0,be,Pt[Et].width,Pt[Et].height,0,fe,de,Pt[Et].data);for(let Jt=0;Jt<It.length;Jt++){const ln=It[Jt].image[Et].image;j?_t&&n.texSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt+1,0,0,ln.width,ln.height,fe,de,ln.data):n.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt+1,be,ln.width,ln.height,0,fe,de,ln.data)}}else{j?_t&&n.texSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,0,0,0,fe,de,Pt[Et]):n.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,0,be,fe,de,Pt[Et]);for(let Jt=0;Jt<It.length;Jt++){const ue=It[Jt];j?_t&&n.texSubImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt+1,0,0,fe,de,ue.image[Et]):n.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Et,Jt+1,be,fe,de,ue.image[Et])}}}x(T)&&w(r.TEXTURE_CUBE_MAP),Lt.__version=At.version,T.onUpdate&&T.onUpdate(T)}L.__version=T.version}function ie(L,T,tt,yt,At,Lt){const zt=c.convert(tt.format,tt.colorSpace),dt=c.convert(tt.type),pt=U(tt.internalFormat,zt,dt,tt.normalized,tt.colorSpace),Bt=a.get(T),Ft=a.get(tt);if(Ft.__renderTarget=T,!Bt.__hasExternalTextures){const Pt=Math.max(1,T.width>>Lt),Ot=Math.max(1,T.height>>Lt);At===r.TEXTURE_3D||At===r.TEXTURE_2D_ARRAY?n.texImage3D(At,Lt,pt,Pt,Ot,T.depth,0,zt,dt,null):n.texImage2D(At,Lt,pt,Pt,Ot,0,zt,dt,null)}n.bindFramebuffer(r.FRAMEBUFFER,L),ye(T)?d.framebufferTexture2DMultisampleEXT(r.FRAMEBUFFER,yt,At,Ft.__webglTexture,0,We(T)):(At===r.TEXTURE_2D||At>=r.TEXTURE_CUBE_MAP_POSITIVE_X&&At<=r.TEXTURE_CUBE_MAP_NEGATIVE_Z)&&r.framebufferTexture2D(r.FRAMEBUFFER,yt,At,Ft.__webglTexture,Lt),n.bindFramebuffer(r.FRAMEBUFFER,null)}function Nt(L,T,tt){if(r.bindRenderbuffer(r.RENDERBUFFER,L),T.depthBuffer){const yt=T.depthTexture,At=yt&&yt.isDepthTexture?yt.type:null,Lt=G(T.stencilBuffer,At),zt=T.stencilBuffer?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT;ye(T)?d.renderbufferStorageMultisampleEXT(r.RENDERBUFFER,We(T),Lt,T.width,T.height):tt?r.renderbufferStorageMultisample(r.RENDERBUFFER,We(T),Lt,T.width,T.height):r.renderbufferStorage(r.RENDERBUFFER,Lt,T.width,T.height),r.framebufferRenderbuffer(r.FRAMEBUFFER,zt,r.RENDERBUFFER,L)}else{const yt=T.textures;for(let At=0;At<yt.length;At++){const Lt=yt[At],zt=c.convert(Lt.format,Lt.colorSpace),dt=c.convert(Lt.type),pt=U(Lt.internalFormat,zt,dt,Lt.normalized,Lt.colorSpace);ye(T)?d.renderbufferStorageMultisampleEXT(r.RENDERBUFFER,We(T),pt,T.width,T.height):tt?r.renderbufferStorageMultisample(r.RENDERBUFFER,We(T),pt,T.width,T.height):r.renderbufferStorage(r.RENDERBUFFER,pt,T.width,T.height)}}r.bindRenderbuffer(r.RENDERBUFFER,null)}function Ht(L,T,tt){const yt=T.isWebGLCubeRenderTarget===!0;if(n.bindFramebuffer(r.FRAMEBUFFER,L),!(T.depthTexture&&T.depthTexture.isDepthTexture))throw new Error("renderTarget.depthTexture must be an instance of THREE.DepthTexture");const At=a.get(T.depthTexture);if(At.__renderTarget=T,(!At.__webglTexture||T.depthTexture.image.width!==T.width||T.depthTexture.image.height!==T.height)&&(T.depthTexture.image.width=T.width,T.depthTexture.image.height=T.height,T.depthTexture.needsUpdate=!0),yt){if(At.__webglInit===void 0&&(At.__webglInit=!0,T.depthTexture.addEventListener("dispose",B)),At.__webglTexture===void 0){At.__webglTexture=r.createTexture(),n.bindTexture(r.TEXTURE_CUBE_MAP,At.__webglTexture),wt(r.TEXTURE_CUBE_MAP,T.depthTexture);const Bt=c.convert(T.depthTexture.format),Ft=c.convert(T.depthTexture.type);let Pt;T.depthTexture.format===Ua?Pt=r.DEPTH_COMPONENT24:T.depthTexture.format===Ks&&(Pt=r.DEPTH24_STENCIL8);for(let Ot=0;Ot<6;Ot++)r.texImage2D(r.TEXTURE_CUBE_MAP_POSITIVE_X+Ot,0,Pt,T.width,T.height,0,Bt,Ft,null)}}else ct(T.depthTexture,0);const Lt=At.__webglTexture,zt=We(T),dt=yt?r.TEXTURE_CUBE_MAP_POSITIVE_X+tt:r.TEXTURE_2D,pt=T.depthTexture.format===Ks?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT;if(T.depthTexture.format===Ua)ye(T)?d.framebufferTexture2DMultisampleEXT(r.FRAMEBUFFER,pt,dt,Lt,0,zt):r.framebufferTexture2D(r.FRAMEBUFFER,pt,dt,Lt,0);else if(T.depthTexture.format===Ks)ye(T)?d.framebufferTexture2DMultisampleEXT(r.FRAMEBUFFER,pt,dt,Lt,0,zt):r.framebufferTexture2D(r.FRAMEBUFFER,pt,dt,Lt,0);else throw new Error("Unknown depthTexture format")}function Ut(L){const T=a.get(L),tt=L.isWebGLCubeRenderTarget===!0;if(T.__boundDepthTexture!==L.depthTexture){const yt=L.depthTexture;if(T.__depthDisposeCallback&&T.__depthDisposeCallback(),yt){const At=()=>{delete T.__boundDepthTexture,delete T.__depthDisposeCallback,yt.removeEventListener("dispose",At)};yt.addEventListener("dispose",At),T.__depthDisposeCallback=At}T.__boundDepthTexture=yt}if(L.depthTexture&&!T.__autoAllocateDepthBuffer)if(tt)for(let yt=0;yt<6;yt++)Ht(T.__webglFramebuffer[yt],L,yt);else{const yt=L.texture.mipmaps;yt&&yt.length>0?Ht(T.__webglFramebuffer[0],L,0):Ht(T.__webglFramebuffer,L,0)}else if(tt){T.__webglDepthbuffer=[];for(let yt=0;yt<6;yt++)if(n.bindFramebuffer(r.FRAMEBUFFER,T.__webglFramebuffer[yt]),T.__webglDepthbuffer[yt]===void 0)T.__webglDepthbuffer[yt]=r.createRenderbuffer(),Nt(T.__webglDepthbuffer[yt],L,!1);else{const At=L.stencilBuffer?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT,Lt=T.__webglDepthbuffer[yt];r.bindRenderbuffer(r.RENDERBUFFER,Lt),r.framebufferRenderbuffer(r.FRAMEBUFFER,At,r.RENDERBUFFER,Lt)}}else{const yt=L.texture.mipmaps;if(yt&&yt.length>0?n.bindFramebuffer(r.FRAMEBUFFER,T.__webglFramebuffer[0]):n.bindFramebuffer(r.FRAMEBUFFER,T.__webglFramebuffer),T.__webglDepthbuffer===void 0)T.__webglDepthbuffer=r.createRenderbuffer(),Nt(T.__webglDepthbuffer,L,!1);else{const At=L.stencilBuffer?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT,Lt=T.__webglDepthbuffer;r.bindRenderbuffer(r.RENDERBUFFER,Lt),r.framebufferRenderbuffer(r.FRAMEBUFFER,At,r.RENDERBUFFER,Lt)}}n.bindFramebuffer(r.FRAMEBUFFER,null)}function Gt(L,T,tt){const yt=a.get(L);T!==void 0&&ie(yt.__webglFramebuffer,L,L.texture,r.COLOR_ATTACHMENT0,r.TEXTURE_2D,0),tt!==void 0&&Ut(L)}function ne(L){const T=L.texture,tt=a.get(L),yt=a.get(T);L.addEventListener("dispose",R);const At=L.textures,Lt=L.isWebGLCubeRenderTarget===!0,zt=At.length>1;if(zt||(yt.__webglTexture===void 0&&(yt.__webglTexture=r.createTexture()),yt.__version=T.version,f.memory.textures++),Lt){tt.__webglFramebuffer=[];for(let dt=0;dt<6;dt++)if(T.mipmaps&&T.mipmaps.length>0){tt.__webglFramebuffer[dt]=[];for(let pt=0;pt<T.mipmaps.length;pt++)tt.__webglFramebuffer[dt][pt]=r.createFramebuffer()}else tt.__webglFramebuffer[dt]=r.createFramebuffer()}else{if(T.mipmaps&&T.mipmaps.length>0){tt.__webglFramebuffer=[];for(let dt=0;dt<T.mipmaps.length;dt++)tt.__webglFramebuffer[dt]=r.createFramebuffer()}else tt.__webglFramebuffer=r.createFramebuffer();if(zt)for(let dt=0,pt=At.length;dt<pt;dt++){const Bt=a.get(At[dt]);Bt.__webglTexture===void 0&&(Bt.__webglTexture=r.createTexture(),f.memory.textures++)}if(L.samples>0&&ye(L)===!1){tt.__webglMultisampledFramebuffer=r.createFramebuffer(),tt.__webglColorRenderbuffer=[],n.bindFramebuffer(r.FRAMEBUFFER,tt.__webglMultisampledFramebuffer);for(let dt=0;dt<At.length;dt++){const pt=At[dt];tt.__webglColorRenderbuffer[dt]=r.createRenderbuffer(),r.bindRenderbuffer(r.RENDERBUFFER,tt.__webglColorRenderbuffer[dt]);const Bt=c.convert(pt.format,pt.colorSpace),Ft=c.convert(pt.type),Pt=U(pt.internalFormat,Bt,Ft,pt.normalized,pt.colorSpace,L.isXRRenderTarget===!0),Ot=We(L);r.renderbufferStorageMultisample(r.RENDERBUFFER,Ot,Pt,L.width,L.height),r.framebufferRenderbuffer(r.FRAMEBUFFER,r.COLOR_ATTACHMENT0+dt,r.RENDERBUFFER,tt.__webglColorRenderbuffer[dt])}r.bindRenderbuffer(r.RENDERBUFFER,null),L.depthBuffer&&(tt.__webglDepthRenderbuffer=r.createRenderbuffer(),Nt(tt.__webglDepthRenderbuffer,L,!0)),n.bindFramebuffer(r.FRAMEBUFFER,null)}}if(Lt){n.bindTexture(r.TEXTURE_CUBE_MAP,yt.__webglTexture),wt(r.TEXTURE_CUBE_MAP,T);for(let dt=0;dt<6;dt++)if(T.mipmaps&&T.mipmaps.length>0)for(let pt=0;pt<T.mipmaps.length;pt++)ie(tt.__webglFramebuffer[dt][pt],L,T,r.COLOR_ATTACHMENT0,r.TEXTURE_CUBE_MAP_POSITIVE_X+dt,pt);else ie(tt.__webglFramebuffer[dt],L,T,r.COLOR_ATTACHMENT0,r.TEXTURE_CUBE_MAP_POSITIVE_X+dt,0);x(T)&&w(r.TEXTURE_CUBE_MAP),n.unbindTexture()}else if(zt){for(let dt=0,pt=At.length;dt<pt;dt++){const Bt=At[dt],Ft=a.get(Bt);let Pt=r.TEXTURE_2D;(L.isWebGL3DRenderTarget||L.isWebGLArrayRenderTarget)&&(Pt=L.isWebGL3DRenderTarget?r.TEXTURE_3D:r.TEXTURE_2D_ARRAY),n.bindTexture(Pt,Ft.__webglTexture),wt(Pt,Bt),ie(tt.__webglFramebuffer,L,Bt,r.COLOR_ATTACHMENT0+dt,Pt,0),x(Bt)&&w(Pt)}n.unbindTexture()}else{let dt=r.TEXTURE_2D;if((L.isWebGL3DRenderTarget||L.isWebGLArrayRenderTarget)&&(dt=L.isWebGL3DRenderTarget?r.TEXTURE_3D:r.TEXTURE_2D_ARRAY),n.bindTexture(dt,yt.__webglTexture),wt(dt,T),T.mipmaps&&T.mipmaps.length>0)for(let pt=0;pt<T.mipmaps.length;pt++)ie(tt.__webglFramebuffer[pt],L,T,r.COLOR_ATTACHMENT0,dt,pt);else ie(tt.__webglFramebuffer,L,T,r.COLOR_ATTACHMENT0,dt,0);x(T)&&w(dt),n.unbindTexture()}L.depthBuffer&&Ut(L)}function Me(L){const T=L.textures;for(let tt=0,yt=T.length;tt<yt;tt++){const At=T[tt];if(x(At)){const Lt=D(L),zt=a.get(At).__webglTexture;n.bindTexture(Lt,zt),w(Lt),n.unbindTexture()}}}const le=[],Ue=[];function W(L){if(L.samples>0){if(ye(L)===!1){const T=L.textures,tt=L.width,yt=L.height;let At=r.COLOR_BUFFER_BIT;const Lt=L.stencilBuffer?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT,zt=a.get(L),dt=T.length>1;if(dt)for(let Bt=0;Bt<T.length;Bt++)n.bindFramebuffer(r.FRAMEBUFFER,zt.__webglMultisampledFramebuffer),r.framebufferRenderbuffer(r.FRAMEBUFFER,r.COLOR_ATTACHMENT0+Bt,r.RENDERBUFFER,null),n.bindFramebuffer(r.FRAMEBUFFER,zt.__webglFramebuffer),r.framebufferTexture2D(r.DRAW_FRAMEBUFFER,r.COLOR_ATTACHMENT0+Bt,r.TEXTURE_2D,null,0);n.bindFramebuffer(r.READ_FRAMEBUFFER,zt.__webglMultisampledFramebuffer);const pt=L.texture.mipmaps;pt&&pt.length>0?n.bindFramebuffer(r.DRAW_FRAMEBUFFER,zt.__webglFramebuffer[0]):n.bindFramebuffer(r.DRAW_FRAMEBUFFER,zt.__webglFramebuffer);for(let Bt=0;Bt<T.length;Bt++){if(L.resolveDepthBuffer&&(L.depthBuffer&&(At|=r.DEPTH_BUFFER_BIT),L.stencilBuffer&&L.resolveStencilBuffer&&(At|=r.STENCIL_BUFFER_BIT)),dt){r.framebufferRenderbuffer(r.READ_FRAMEBUFFER,r.COLOR_ATTACHMENT0,r.RENDERBUFFER,zt.__webglColorRenderbuffer[Bt]);const Ft=a.get(T[Bt]).__webglTexture;r.framebufferTexture2D(r.DRAW_FRAMEBUFFER,r.COLOR_ATTACHMENT0,r.TEXTURE_2D,Ft,0)}r.blitFramebuffer(0,0,tt,yt,0,0,tt,yt,At,r.NEAREST),m===!0&&(le.length=0,Ue.length=0,le.push(r.COLOR_ATTACHMENT0+Bt),L.depthBuffer&&L.resolveDepthBuffer===!1&&(le.push(Lt),Ue.push(Lt),r.invalidateFramebuffer(r.DRAW_FRAMEBUFFER,Ue)),r.invalidateFramebuffer(r.READ_FRAMEBUFFER,le))}if(n.bindFramebuffer(r.READ_FRAMEBUFFER,null),n.bindFramebuffer(r.DRAW_FRAMEBUFFER,null),dt)for(let Bt=0;Bt<T.length;Bt++){n.bindFramebuffer(r.FRAMEBUFFER,zt.__webglMultisampledFramebuffer),r.framebufferRenderbuffer(r.FRAMEBUFFER,r.COLOR_ATTACHMENT0+Bt,r.RENDERBUFFER,zt.__webglColorRenderbuffer[Bt]);const Ft=a.get(T[Bt]).__webglTexture;n.bindFramebuffer(r.FRAMEBUFFER,zt.__webglFramebuffer),r.framebufferTexture2D(r.DRAW_FRAMEBUFFER,r.COLOR_ATTACHMENT0+Bt,r.TEXTURE_2D,Ft,0)}n.bindFramebuffer(r.DRAW_FRAMEBUFFER,zt.__webglMultisampledFramebuffer)}else if(L.depthBuffer&&L.resolveDepthBuffer===!1&&m){const T=L.stencilBuffer?r.DEPTH_STENCIL_ATTACHMENT:r.DEPTH_ATTACHMENT;r.invalidateFramebuffer(r.DRAW_FRAMEBUFFER,[T])}}}function We(L){return Math.min(l.maxSamples,L.samples)}function ye(L){const T=a.get(L);return L.samples>0&&t.has("WEBGL_multisampled_render_to_texture")===!0&&T.__useRenderToTexture!==!1}function qe(L){const T=f.render.frame;g.get(L)!==T&&(g.set(L,T),L.update())}function Dt(L,T){const tt=L.colorSpace,yt=L.format,At=L.type;return L.isCompressedTexture===!0||L.isVideoTexture===!0||tt!==Eu&&tt!==gs&&(De.getTransfer(tt)===je?(yt!==Hi||At!==pi)&&ce("WebGLTextures: sRGB encoded textures have to use RGBAFormat and UnsignedByteType."):Ne("WebGLTextures: Unsupported texture color space:",tt)),T}function an(L){return typeof HTMLImageElement<"u"&&L instanceof HTMLImageElement?(h.width=L.naturalWidth||L.width,h.height=L.naturalHeight||L.height):typeof VideoFrame<"u"&&L instanceof VideoFrame?(h.width=L.displayWidth,h.height=L.displayHeight):(h.width=L.width,h.height=L.height),h}this.allocateTextureUnit=P,this.resetTextureUnits=ht,this.getTextureUnits=gt,this.setTextureUnits=q,this.setTexture2D=ct,this.setTexture2DArray=J,this.setTexture3D=xt,this.setTextureCube=I,this.rebindTextures=Gt,this.setupRenderTarget=ne,this.updateRenderTargetMipmap=Me,this.updateMultisampleRenderTarget=W,this.setupDepthRenderbuffer=Ut,this.setupFrameBufferTexture=ie,this.useMultisampledRTT=ye,this.isReversedDepthBuffer=function(){return n.buffers.depth.getReversed()}}function gR(r,t){function n(a,l=gs){let c;const f=De.getTransfer(l);if(a===pi)return r.UNSIGNED_BYTE;if(a===Bp)return r.UNSIGNED_SHORT_4_4_4_4;if(a===Fp)return r.UNSIGNED_SHORT_5_5_5_1;if(a===Rx)return r.UNSIGNED_INT_5_9_9_9_REV;if(a===Cx)return r.UNSIGNED_INT_10F_11F_11F_REV;if(a===Ax)return r.BYTE;if(a===wx)return r.SHORT;if(a===Ml)return r.UNSIGNED_SHORT;if(a===zp)return r.INT;if(a===ea)return r.UNSIGNED_INT;if(a===Qi)return r.FLOAT;if(a===Da)return r.HALF_FLOAT;if(a===Nx)return r.ALPHA;if(a===Dx)return r.RGB;if(a===Hi)return r.RGBA;if(a===Ua)return r.DEPTH_COMPONENT;if(a===Ks)return r.DEPTH_STENCIL;if(a===Ux)return r.RED;if(a===Hp)return r.RED_INTEGER;if(a===$s)return r.RG;if(a===Gp)return r.RG_INTEGER;if(a===Vp)return r.RGBA_INTEGER;if(a===mu||a===gu||a===_u||a===vu)if(f===je)if(c=t.get("WEBGL_compressed_texture_s3tc_srgb"),c!==null){if(a===mu)return c.COMPRESSED_SRGB_S3TC_DXT1_EXT;if(a===gu)return c.COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT;if(a===_u)return c.COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT;if(a===vu)return c.COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT}else return null;else if(c=t.get("WEBGL_compressed_texture_s3tc"),c!==null){if(a===mu)return c.COMPRESSED_RGB_S3TC_DXT1_EXT;if(a===gu)return c.COMPRESSED_RGBA_S3TC_DXT1_EXT;if(a===_u)return c.COMPRESSED_RGBA_S3TC_DXT3_EXT;if(a===vu)return c.COMPRESSED_RGBA_S3TC_DXT5_EXT}else return null;if(a===qh||a===Yh||a===Zh||a===Kh)if(c=t.get("WEBGL_compressed_texture_pvrtc"),c!==null){if(a===qh)return c.COMPRESSED_RGB_PVRTC_4BPPV1_IMG;if(a===Yh)return c.COMPRESSED_RGB_PVRTC_2BPPV1_IMG;if(a===Zh)return c.COMPRESSED_RGBA_PVRTC_4BPPV1_IMG;if(a===Kh)return c.COMPRESSED_RGBA_PVRTC_2BPPV1_IMG}else return null;if(a===Qh||a===Jh||a===$h||a===tp||a===ep||a===Mu||a===np)if(c=t.get("WEBGL_compressed_texture_etc"),c!==null){if(a===Qh||a===Jh)return f===je?c.COMPRESSED_SRGB8_ETC2:c.COMPRESSED_RGB8_ETC2;if(a===$h)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ETC2_EAC:c.COMPRESSED_RGBA8_ETC2_EAC;if(a===tp)return c.COMPRESSED_R11_EAC;if(a===ep)return c.COMPRESSED_SIGNED_R11_EAC;if(a===Mu)return c.COMPRESSED_RG11_EAC;if(a===np)return c.COMPRESSED_SIGNED_RG11_EAC}else return null;if(a===ip||a===ap||a===sp||a===rp||a===op||a===lp||a===cp||a===up||a===fp||a===dp||a===hp||a===pp||a===mp||a===gp)if(c=t.get("WEBGL_compressed_texture_astc"),c!==null){if(a===ip)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_4x4_KHR:c.COMPRESSED_RGBA_ASTC_4x4_KHR;if(a===ap)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_5x4_KHR:c.COMPRESSED_RGBA_ASTC_5x4_KHR;if(a===sp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_5x5_KHR:c.COMPRESSED_RGBA_ASTC_5x5_KHR;if(a===rp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_6x5_KHR:c.COMPRESSED_RGBA_ASTC_6x5_KHR;if(a===op)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_6x6_KHR:c.COMPRESSED_RGBA_ASTC_6x6_KHR;if(a===lp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_8x5_KHR:c.COMPRESSED_RGBA_ASTC_8x5_KHR;if(a===cp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_8x6_KHR:c.COMPRESSED_RGBA_ASTC_8x6_KHR;if(a===up)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_8x8_KHR:c.COMPRESSED_RGBA_ASTC_8x8_KHR;if(a===fp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_10x5_KHR:c.COMPRESSED_RGBA_ASTC_10x5_KHR;if(a===dp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_10x6_KHR:c.COMPRESSED_RGBA_ASTC_10x6_KHR;if(a===hp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_10x8_KHR:c.COMPRESSED_RGBA_ASTC_10x8_KHR;if(a===pp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_10x10_KHR:c.COMPRESSED_RGBA_ASTC_10x10_KHR;if(a===mp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_12x10_KHR:c.COMPRESSED_RGBA_ASTC_12x10_KHR;if(a===gp)return f===je?c.COMPRESSED_SRGB8_ALPHA8_ASTC_12x12_KHR:c.COMPRESSED_RGBA_ASTC_12x12_KHR}else return null;if(a===_p||a===vp||a===xp)if(c=t.get("EXT_texture_compression_bptc"),c!==null){if(a===_p)return f===je?c.COMPRESSED_SRGB_ALPHA_BPTC_UNORM_EXT:c.COMPRESSED_RGBA_BPTC_UNORM_EXT;if(a===vp)return c.COMPRESSED_RGB_BPTC_SIGNED_FLOAT_EXT;if(a===xp)return c.COMPRESSED_RGB_BPTC_UNSIGNED_FLOAT_EXT}else return null;if(a===yp||a===Sp||a===bu||a===Mp)if(c=t.get("EXT_texture_compression_rgtc"),c!==null){if(a===yp)return c.COMPRESSED_RED_RGTC1_EXT;if(a===Sp)return c.COMPRESSED_SIGNED_RED_RGTC1_EXT;if(a===bu)return c.COMPRESSED_RED_GREEN_RGTC2_EXT;if(a===Mp)return c.COMPRESSED_SIGNED_RED_GREEN_RGTC2_EXT}else return null;return a===bl?r.UNSIGNED_INT_24_8:r[a]!==void 0?r[a]:null}return{convert:n}}const _R=`
void main() {

	gl_Position = vec4( position, 1.0 );

}`,vR=`
uniform sampler2DArray depthColor;
uniform float depthWidth;
uniform float depthHeight;

void main() {

	vec2 coord = vec2( gl_FragCoord.x / depthWidth, gl_FragCoord.y / depthHeight );

	if ( coord.x >= 1.0 ) {

		gl_FragDepth = texture( depthColor, vec3( coord.x - 1.0, coord.y, 1 ) ).r;

	} else {

		gl_FragDepth = texture( depthColor, vec3( coord.x, coord.y, 0 ) ).r;

	}

}`;class xR{constructor(){this.texture=null,this.mesh=null,this.depthNear=0,this.depthFar=0}init(t,n){if(this.texture===null){const a=new Gx(t.texture);(t.depthNear!==n.depthNear||t.depthFar!==n.depthFar)&&(this.depthNear=t.depthNear,this.depthFar=t.depthFar),this.texture=a}}getMesh(t){if(this.texture!==null&&this.mesh===null){const n=t.cameras[0].viewport,a=new na({vertexShader:_R,fragmentShader:vR,uniforms:{depthColor:{value:this.texture},depthWidth:{value:n.z},depthHeight:{value:n.w}}});this.mesh=new Gn(new Pu(20,20),a)}return this.mesh}reset(){this.texture=null,this.mesh=null}getDepthTexture(){return this.texture}}class yR extends tr{constructor(t,n){super();const a=this;let l=null,c=1,f=null,d="local-floor",m=1,h=null,g=null,_=null,v=null,y=null,E=null;const A=typeof XRWebGLBinding<"u",S=new xR,x={},w=n.getContextAttributes();let D=null,U=null;const G=[],O=[],B=new ee;let R=null;const z=new hi;z.viewport=new fn;const K=new hi;K.viewport=new fn;const V=[z,K],$=new CE;let ht=null,gt=null;this.cameraAutoUpdate=!0,this.enabled=!1,this.isPresenting=!1,this.getController=function(st){let bt=G[st];return bt===void 0&&(bt=new sh,G[st]=bt),bt.getTargetRaySpace()},this.getControllerGrip=function(st){let bt=G[st];return bt===void 0&&(bt=new sh,G[st]=bt),bt.getGripSpace()},this.getHand=function(st){let bt=G[st];return bt===void 0&&(bt=new sh,G[st]=bt),bt.getHandSpace()};function q(st){const bt=O.indexOf(st.inputSource);if(bt===-1)return;const Tt=G[bt];Tt!==void 0&&(Tt.update(st.inputSource,st.frame,h||f),Tt.dispatchEvent({type:st.type,data:st.inputSource}))}function P(){l.removeEventListener("select",q),l.removeEventListener("selectstart",q),l.removeEventListener("selectend",q),l.removeEventListener("squeeze",q),l.removeEventListener("squeezestart",q),l.removeEventListener("squeezeend",q),l.removeEventListener("end",P),l.removeEventListener("inputsourceschange",F);for(let st=0;st<G.length;st++){const bt=O[st];bt!==null&&(O[st]=null,G[st].disconnect(bt))}ht=null,gt=null,S.reset();for(const st in x)delete x[st];t.setRenderTarget(D),y=null,v=null,_=null,l=null,U=null,wt.stop(),a.isPresenting=!1,t.setPixelRatio(R),t.setSize(B.width,B.height,!1),a.dispatchEvent({type:"sessionend"})}this.setFramebufferScaleFactor=function(st){c=st,a.isPresenting===!0&&ce("WebXRManager: Cannot change framebuffer scale while presenting.")},this.setReferenceSpaceType=function(st){d=st,a.isPresenting===!0&&ce("WebXRManager: Cannot change reference space type while presenting.")},this.getReferenceSpace=function(){return h||f},this.setReferenceSpace=function(st){h=st},this.getBaseLayer=function(){return v!==null?v:y},this.getBinding=function(){return _===null&&A&&(_=new XRWebGLBinding(l,n)),_},this.getFrame=function(){return E},this.getSession=function(){return l},this.setSession=async function(st){if(l=st,l!==null){if(D=t.getRenderTarget(),l.addEventListener("select",q),l.addEventListener("selectstart",q),l.addEventListener("selectend",q),l.addEventListener("squeeze",q),l.addEventListener("squeezestart",q),l.addEventListener("squeezeend",q),l.addEventListener("end",P),l.addEventListener("inputsourceschange",F),w.xrCompatible!==!0&&await n.makeXRCompatible(),R=t.getPixelRatio(),t.getSize(B),A&&"createProjectionLayer"in XRWebGLBinding.prototype){let Tt=null,Wt=null,re=null;w.depth&&(re=w.stencil?n.DEPTH24_STENCIL8:n.DEPTH_COMPONENT24,Tt=w.stencil?Ks:Ua,Wt=w.stencil?bl:ea);const ie={colorFormat:n.RGBA8,depthFormat:re,scaleFactor:c};_=this.getBinding(),v=_.createProjectionLayer(ie),l.updateRenderState({layers:[v]}),t.setPixelRatio(1),t.setSize(v.textureWidth,v.textureHeight,!1),U=new ta(v.textureWidth,v.textureHeight,{format:Hi,type:pi,depthTexture:new oo(v.textureWidth,v.textureHeight,Wt,void 0,void 0,void 0,void 0,void 0,void 0,Tt),stencilBuffer:w.stencil,colorSpace:t.outputColorSpace,samples:w.antialias?4:0,resolveDepthBuffer:v.ignoreDepthValues===!1,resolveStencilBuffer:v.ignoreDepthValues===!1})}else{const Tt={antialias:w.antialias,alpha:!0,depth:w.depth,stencil:w.stencil,framebufferScaleFactor:c};y=new XRWebGLLayer(l,n,Tt),l.updateRenderState({baseLayer:y}),t.setPixelRatio(1),t.setSize(y.framebufferWidth,y.framebufferHeight,!1),U=new ta(y.framebufferWidth,y.framebufferHeight,{format:Hi,type:pi,colorSpace:t.outputColorSpace,stencilBuffer:w.stencil,resolveDepthBuffer:y.ignoreDepthValues===!1,resolveStencilBuffer:y.ignoreDepthValues===!1})}U.isXRRenderTarget=!0,this.setFoveation(m),h=null,f=await l.requestReferenceSpace(d),wt.setContext(l),wt.start(),a.isPresenting=!0,a.dispatchEvent({type:"sessionstart"})}},this.getEnvironmentBlendMode=function(){if(l!==null)return l.environmentBlendMode},this.getDepthTexture=function(){return S.getDepthTexture()};function F(st){for(let bt=0;bt<st.removed.length;bt++){const Tt=st.removed[bt],Wt=O.indexOf(Tt);Wt>=0&&(O[Wt]=null,G[Wt].disconnect(Tt))}for(let bt=0;bt<st.added.length;bt++){const Tt=st.added[bt];let Wt=O.indexOf(Tt);if(Wt===-1){for(let ie=0;ie<G.length;ie++)if(ie>=O.length){O.push(Tt),Wt=ie;break}else if(O[ie]===null){O[ie]=Tt,Wt=ie;break}if(Wt===-1)break}const re=G[Wt];re&&re.connect(Tt)}}const ct=new k,J=new k;function xt(st,bt,Tt){ct.setFromMatrixPosition(bt.matrixWorld),J.setFromMatrixPosition(Tt.matrixWorld);const Wt=ct.distanceTo(J),re=bt.projectionMatrix.elements,ie=Tt.projectionMatrix.elements,Nt=re[14]/(re[10]-1),Ht=re[14]/(re[10]+1),Ut=(re[9]+1)/re[5],Gt=(re[9]-1)/re[5],ne=(re[8]-1)/re[0],Me=(ie[8]+1)/ie[0],le=Nt*ne,Ue=Nt*Me,W=Wt/(-ne+Me),We=W*-ne;if(bt.matrixWorld.decompose(st.position,st.quaternion,st.scale),st.translateX(We),st.translateZ(W),st.matrixWorld.compose(st.position,st.quaternion,st.scale),st.matrixWorldInverse.copy(st.matrixWorld).invert(),re[10]===-1)st.projectionMatrix.copy(bt.projectionMatrix),st.projectionMatrixInverse.copy(bt.projectionMatrixInverse);else{const ye=Nt+W,qe=Ht+W,Dt=le-We,an=Ue+(Wt-We),L=Ut*Ht/qe*ye,T=Gt*Ht/qe*ye;st.projectionMatrix.makePerspective(Dt,an,L,T,ye,qe),st.projectionMatrixInverse.copy(st.projectionMatrix).invert()}}function I(st,bt){bt===null?st.matrixWorld.copy(st.matrix):st.matrixWorld.multiplyMatrices(bt.matrixWorld,st.matrix),st.matrixWorldInverse.copy(st.matrixWorld).invert()}this.updateCamera=function(st){if(l===null)return;let bt=st.near,Tt=st.far;S.texture!==null&&(S.depthNear>0&&(bt=S.depthNear),S.depthFar>0&&(Tt=S.depthFar)),$.near=K.near=z.near=bt,$.far=K.far=z.far=Tt,(ht!==$.near||gt!==$.far)&&(l.updateRenderState({depthNear:$.near,depthFar:$.far}),ht=$.near,gt=$.far),$.layers.mask=st.layers.mask|6,z.layers.mask=$.layers.mask&-5,K.layers.mask=$.layers.mask&-3;const Wt=st.parent,re=$.cameras;I($,Wt);for(let ie=0;ie<re.length;ie++)I(re[ie],Wt);re.length===2?xt($,z,K):$.projectionMatrix.copy(z.projectionMatrix),Q(st,$,Wt)};function Q(st,bt,Tt){Tt===null?st.matrix.copy(bt.matrixWorld):(st.matrix.copy(Tt.matrixWorld),st.matrix.invert(),st.matrix.multiply(bt.matrixWorld)),st.matrix.decompose(st.position,st.quaternion,st.scale),st.updateMatrixWorld(!0),st.projectionMatrix.copy(bt.projectionMatrix),st.projectionMatrixInverse.copy(bt.projectionMatrixInverse),st.isPerspectiveCamera&&(st.fov=Tl*2*Math.atan(1/st.projectionMatrix.elements[5]),st.zoom=1)}this.getCamera=function(){return $},this.getFoveation=function(){if(!(v===null&&y===null))return m},this.setFoveation=function(st){m=st,v!==null&&(v.fixedFoveation=st),y!==null&&y.fixedFoveation!==void 0&&(y.fixedFoveation=st)},this.hasDepthSensing=function(){return S.texture!==null},this.getDepthSensingMesh=function(){return S.getMesh($)},this.getCameraTexture=function(st){return x[st]};let Mt=null;function Rt(st,bt){if(g=bt.getViewerPose(h||f),E=bt,g!==null){const Tt=g.views;y!==null&&(t.setRenderTargetFramebuffer(U,y.framebuffer),t.setRenderTarget(U));let Wt=!1;Tt.length!==$.cameras.length&&($.cameras.length=0,Wt=!0);for(let Ht=0;Ht<Tt.length;Ht++){const Ut=Tt[Ht];let Gt=null;if(y!==null)Gt=y.getViewport(Ut);else{const Me=_.getViewSubImage(v,Ut);Gt=Me.viewport,Ht===0&&(t.setRenderTargetTextures(U,Me.colorTexture,Me.depthStencilTexture),t.setRenderTarget(U))}let ne=V[Ht];ne===void 0&&(ne=new hi,ne.layers.enable(Ht),ne.viewport=new fn,V[Ht]=ne),ne.matrix.fromArray(Ut.transform.matrix),ne.matrix.decompose(ne.position,ne.quaternion,ne.scale),ne.projectionMatrix.fromArray(Ut.projectionMatrix),ne.projectionMatrixInverse.copy(ne.projectionMatrix).invert(),ne.viewport.set(Gt.x,Gt.y,Gt.width,Gt.height),Ht===0&&($.matrix.copy(ne.matrix),$.matrix.decompose($.position,$.quaternion,$.scale)),Wt===!0&&$.cameras.push(ne)}const re=l.enabledFeatures;if(re&&re.includes("depth-sensing")&&l.depthUsage=="gpu-optimized"&&A){_=a.getBinding();const Ht=_.getDepthInformation(Tt[0]);Ht&&Ht.isValid&&Ht.texture&&S.init(Ht,l.renderState)}if(re&&re.includes("camera-access")&&A){t.state.unbindTexture(),_=a.getBinding();for(let Ht=0;Ht<Tt.length;Ht++){const Ut=Tt[Ht].camera;if(Ut){let Gt=x[Ut];Gt||(Gt=new Gx,x[Ut]=Gt);const ne=_.getCameraImage(Ut);Gt.sourceTexture=ne}}}}for(let Tt=0;Tt<G.length;Tt++){const Wt=O[Tt],re=G[Tt];Wt!==null&&re!==void 0&&re.update(Wt,bt,h||f)}Mt&&Mt(st,bt),bt.detectedPlanes&&a.dispatchEvent({type:"planesdetected",data:bt}),E=null}const wt=new Kx;wt.setAnimationLoop(Rt),this.setAnimationLoop=function(st){Mt=st},this.dispose=function(){}}}const SR=new tn,iy=new pe;iy.set(-1,0,0,0,1,0,0,0,1);function MR(r,t){function n(S,x){S.matrixAutoUpdate===!0&&S.updateMatrix(),x.value.copy(S.matrix)}function a(S,x){x.color.getRGB(S.fogColor.value,Xx(r)),x.isFog?(S.fogNear.value=x.near,S.fogFar.value=x.far):x.isFogExp2&&(S.fogDensity.value=x.density)}function l(S,x,w,D,U){x.isNodeMaterial?x.uniformsNeedUpdate=!1:x.isMeshBasicMaterial?c(S,x):x.isMeshLambertMaterial?(c(S,x),x.envMap&&(S.envMapIntensity.value=x.envMapIntensity)):x.isMeshToonMaterial?(c(S,x),_(S,x)):x.isMeshPhongMaterial?(c(S,x),g(S,x),x.envMap&&(S.envMapIntensity.value=x.envMapIntensity)):x.isMeshStandardMaterial?(c(S,x),v(S,x),x.isMeshPhysicalMaterial&&y(S,x,U)):x.isMeshMatcapMaterial?(c(S,x),E(S,x)):x.isMeshDepthMaterial?c(S,x):x.isMeshDistanceMaterial?(c(S,x),A(S,x)):x.isMeshNormalMaterial?c(S,x):x.isLineBasicMaterial?(f(S,x),x.isLineDashedMaterial&&d(S,x)):x.isPointsMaterial?m(S,x,w,D):x.isSpriteMaterial?h(S,x):x.isShadowMaterial?(S.color.value.copy(x.color),S.opacity.value=x.opacity):x.isShaderMaterial&&(x.uniformsNeedUpdate=!1)}function c(S,x){S.opacity.value=x.opacity,x.color&&S.diffuse.value.copy(x.color),x.emissive&&S.emissive.value.copy(x.emissive).multiplyScalar(x.emissiveIntensity),x.map&&(S.map.value=x.map,n(x.map,S.mapTransform)),x.alphaMap&&(S.alphaMap.value=x.alphaMap,n(x.alphaMap,S.alphaMapTransform)),x.bumpMap&&(S.bumpMap.value=x.bumpMap,n(x.bumpMap,S.bumpMapTransform),S.bumpScale.value=x.bumpScale,x.side===ti&&(S.bumpScale.value*=-1)),x.normalMap&&(S.normalMap.value=x.normalMap,n(x.normalMap,S.normalMapTransform),S.normalScale.value.copy(x.normalScale),x.side===ti&&S.normalScale.value.negate()),x.displacementMap&&(S.displacementMap.value=x.displacementMap,n(x.displacementMap,S.displacementMapTransform),S.displacementScale.value=x.displacementScale,S.displacementBias.value=x.displacementBias),x.emissiveMap&&(S.emissiveMap.value=x.emissiveMap,n(x.emissiveMap,S.emissiveMapTransform)),x.specularMap&&(S.specularMap.value=x.specularMap,n(x.specularMap,S.specularMapTransform)),x.alphaTest>0&&(S.alphaTest.value=x.alphaTest);const w=t.get(x),D=w.envMap,U=w.envMapRotation;D&&(S.envMap.value=D,S.envMapRotation.value.setFromMatrix4(SR.makeRotationFromEuler(U)).transpose(),D.isCubeTexture&&D.isRenderTargetTexture===!1&&S.envMapRotation.value.premultiply(iy),S.reflectivity.value=x.reflectivity,S.ior.value=x.ior,S.refractionRatio.value=x.refractionRatio),x.lightMap&&(S.lightMap.value=x.lightMap,S.lightMapIntensity.value=x.lightMapIntensity,n(x.lightMap,S.lightMapTransform)),x.aoMap&&(S.aoMap.value=x.aoMap,S.aoMapIntensity.value=x.aoMapIntensity,n(x.aoMap,S.aoMapTransform))}function f(S,x){S.diffuse.value.copy(x.color),S.opacity.value=x.opacity,x.map&&(S.map.value=x.map,n(x.map,S.mapTransform))}function d(S,x){S.dashSize.value=x.dashSize,S.totalSize.value=x.dashSize+x.gapSize,S.scale.value=x.scale}function m(S,x,w,D){S.diffuse.value.copy(x.color),S.opacity.value=x.opacity,S.size.value=x.size*w,S.scale.value=D*.5,x.map&&(S.map.value=x.map,n(x.map,S.uvTransform)),x.alphaMap&&(S.alphaMap.value=x.alphaMap,n(x.alphaMap,S.alphaMapTransform)),x.alphaTest>0&&(S.alphaTest.value=x.alphaTest)}function h(S,x){S.diffuse.value.copy(x.color),S.opacity.value=x.opacity,S.rotation.value=x.rotation,x.map&&(S.map.value=x.map,n(x.map,S.mapTransform)),x.alphaMap&&(S.alphaMap.value=x.alphaMap,n(x.alphaMap,S.alphaMapTransform)),x.alphaTest>0&&(S.alphaTest.value=x.alphaTest)}function g(S,x){S.specular.value.copy(x.specular),S.shininess.value=Math.max(x.shininess,1e-4)}function _(S,x){x.gradientMap&&(S.gradientMap.value=x.gradientMap)}function v(S,x){S.metalness.value=x.metalness,x.metalnessMap&&(S.metalnessMap.value=x.metalnessMap,n(x.metalnessMap,S.metalnessMapTransform)),S.roughness.value=x.roughness,x.roughnessMap&&(S.roughnessMap.value=x.roughnessMap,n(x.roughnessMap,S.roughnessMapTransform)),x.envMap&&(S.envMapIntensity.value=x.envMapIntensity)}function y(S,x,w){S.ior.value=x.ior,x.sheen>0&&(S.sheenColor.value.copy(x.sheenColor).multiplyScalar(x.sheen),S.sheenRoughness.value=x.sheenRoughness,x.sheenColorMap&&(S.sheenColorMap.value=x.sheenColorMap,n(x.sheenColorMap,S.sheenColorMapTransform)),x.sheenRoughnessMap&&(S.sheenRoughnessMap.value=x.sheenRoughnessMap,n(x.sheenRoughnessMap,S.sheenRoughnessMapTransform))),x.clearcoat>0&&(S.clearcoat.value=x.clearcoat,S.clearcoatRoughness.value=x.clearcoatRoughness,x.clearcoatMap&&(S.clearcoatMap.value=x.clearcoatMap,n(x.clearcoatMap,S.clearcoatMapTransform)),x.clearcoatRoughnessMap&&(S.clearcoatRoughnessMap.value=x.clearcoatRoughnessMap,n(x.clearcoatRoughnessMap,S.clearcoatRoughnessMapTransform)),x.clearcoatNormalMap&&(S.clearcoatNormalMap.value=x.clearcoatNormalMap,n(x.clearcoatNormalMap,S.clearcoatNormalMapTransform),S.clearcoatNormalScale.value.copy(x.clearcoatNormalScale),x.side===ti&&S.clearcoatNormalScale.value.negate())),x.dispersion>0&&(S.dispersion.value=x.dispersion),x.iridescence>0&&(S.iridescence.value=x.iridescence,S.iridescenceIOR.value=x.iridescenceIOR,S.iridescenceThicknessMinimum.value=x.iridescenceThicknessRange[0],S.iridescenceThicknessMaximum.value=x.iridescenceThicknessRange[1],x.iridescenceMap&&(S.iridescenceMap.value=x.iridescenceMap,n(x.iridescenceMap,S.iridescenceMapTransform)),x.iridescenceThicknessMap&&(S.iridescenceThicknessMap.value=x.iridescenceThicknessMap,n(x.iridescenceThicknessMap,S.iridescenceThicknessMapTransform))),x.transmission>0&&(S.transmission.value=x.transmission,S.transmissionSamplerMap.value=w.texture,S.transmissionSamplerSize.value.set(w.width,w.height),x.transmissionMap&&(S.transmissionMap.value=x.transmissionMap,n(x.transmissionMap,S.transmissionMapTransform)),S.thickness.value=x.thickness,x.thicknessMap&&(S.thicknessMap.value=x.thicknessMap,n(x.thicknessMap,S.thicknessMapTransform)),S.attenuationDistance.value=x.attenuationDistance,S.attenuationColor.value.copy(x.attenuationColor)),x.anisotropy>0&&(S.anisotropyVector.value.set(x.anisotropy*Math.cos(x.anisotropyRotation),x.anisotropy*Math.sin(x.anisotropyRotation)),x.anisotropyMap&&(S.anisotropyMap.value=x.anisotropyMap,n(x.anisotropyMap,S.anisotropyMapTransform))),S.specularIntensity.value=x.specularIntensity,S.specularColor.value.copy(x.specularColor),x.specularColorMap&&(S.specularColorMap.value=x.specularColorMap,n(x.specularColorMap,S.specularColorMapTransform)),x.specularIntensityMap&&(S.specularIntensityMap.value=x.specularIntensityMap,n(x.specularIntensityMap,S.specularIntensityMapTransform))}function E(S,x){x.matcap&&(S.matcap.value=x.matcap)}function A(S,x){const w=t.get(x).light;S.referencePosition.value.setFromMatrixPosition(w.matrixWorld),S.nearDistance.value=w.shadow.camera.near,S.farDistance.value=w.shadow.camera.far}return{refreshFogUniforms:a,refreshMaterialUniforms:l}}function bR(r,t,n,a){let l={},c={},f=[];const d=r.getParameter(r.MAX_UNIFORM_BUFFER_BINDINGS);function m(w,D){const U=D.program;a.uniformBlockBinding(w,U)}function h(w,D){let U=l[w.id];U===void 0&&(E(w),U=g(w),l[w.id]=U,w.addEventListener("dispose",S));const G=D.program;a.updateUBOMapping(w,G);const O=t.render.frame;c[w.id]!==O&&(v(w),c[w.id]=O)}function g(w){const D=_();w.__bindingPointIndex=D;const U=r.createBuffer(),G=w.__size,O=w.usage;return r.bindBuffer(r.UNIFORM_BUFFER,U),r.bufferData(r.UNIFORM_BUFFER,G,O),r.bindBuffer(r.UNIFORM_BUFFER,null),r.bindBufferBase(r.UNIFORM_BUFFER,D,U),U}function _(){for(let w=0;w<d;w++)if(f.indexOf(w)===-1)return f.push(w),w;return Ne("WebGLRenderer: Maximum number of simultaneously usable uniforms groups reached."),0}function v(w){const D=l[w.id],U=w.uniforms,G=w.__cache;r.bindBuffer(r.UNIFORM_BUFFER,D);for(let O=0,B=U.length;O<B;O++){const R=Array.isArray(U[O])?U[O]:[U[O]];for(let z=0,K=R.length;z<K;z++){const V=R[z];if(y(V,O,z,G)===!0){const $=V.__offset,ht=Array.isArray(V.value)?V.value:[V.value];let gt=0;for(let q=0;q<ht.length;q++){const P=ht[q],F=A(P);typeof P=="number"||typeof P=="boolean"?(V.__data[0]=P,r.bufferSubData(r.UNIFORM_BUFFER,$+gt,V.__data)):P.isMatrix3?(V.__data[0]=P.elements[0],V.__data[1]=P.elements[1],V.__data[2]=P.elements[2],V.__data[3]=0,V.__data[4]=P.elements[3],V.__data[5]=P.elements[4],V.__data[6]=P.elements[5],V.__data[7]=0,V.__data[8]=P.elements[6],V.__data[9]=P.elements[7],V.__data[10]=P.elements[8],V.__data[11]=0):ArrayBuffer.isView(P)?V.__data.set(new P.constructor(P.buffer,P.byteOffset,V.__data.length)):(P.toArray(V.__data,gt),gt+=F.storage/Float32Array.BYTES_PER_ELEMENT)}r.bufferSubData(r.UNIFORM_BUFFER,$,V.__data)}}}r.bindBuffer(r.UNIFORM_BUFFER,null)}function y(w,D,U,G){const O=w.value,B=D+"_"+U;if(G[B]===void 0)return typeof O=="number"||typeof O=="boolean"?G[B]=O:ArrayBuffer.isView(O)?G[B]=O.slice():G[B]=O.clone(),!0;{const R=G[B];if(typeof O=="number"||typeof O=="boolean"){if(R!==O)return G[B]=O,!0}else{if(ArrayBuffer.isView(O))return!0;if(R.equals(O)===!1)return R.copy(O),!0}}return!1}function E(w){const D=w.uniforms;let U=0;const G=16;for(let B=0,R=D.length;B<R;B++){const z=Array.isArray(D[B])?D[B]:[D[B]];for(let K=0,V=z.length;K<V;K++){const $=z[K],ht=Array.isArray($.value)?$.value:[$.value];for(let gt=0,q=ht.length;gt<q;gt++){const P=ht[gt],F=A(P),ct=U%G,J=ct%F.boundary,xt=ct+J;U+=J,xt!==0&&G-xt<F.storage&&(U+=G-xt),$.__data=new Float32Array(F.storage/Float32Array.BYTES_PER_ELEMENT),$.__offset=U,U+=F.storage}}}const O=U%G;return O>0&&(U+=G-O),w.__size=U,w.__cache={},this}function A(w){const D={boundary:0,storage:0};return typeof w=="number"||typeof w=="boolean"?(D.boundary=4,D.storage=4):w.isVector2?(D.boundary=8,D.storage=8):w.isVector3||w.isColor?(D.boundary=16,D.storage=12):w.isVector4?(D.boundary=16,D.storage=16):w.isMatrix3?(D.boundary=48,D.storage=48):w.isMatrix4?(D.boundary=64,D.storage=64):w.isTexture?ce("WebGLRenderer: Texture samplers can not be part of an uniforms group."):ArrayBuffer.isView(w)?(D.boundary=16,D.storage=w.byteLength):ce("WebGLRenderer: Unsupported uniform value type.",w),D}function S(w){const D=w.target;D.removeEventListener("dispose",S);const U=f.indexOf(D.__bindingPointIndex);f.splice(U,1),r.deleteBuffer(l[D.id]),delete l[D.id],delete c[D.id]}function x(){for(const w in l)r.deleteBuffer(l[w]);f=[],l={},c={}}return{bind:m,update:h,dispose:x}}const ER=new Uint16Array([12469,15057,12620,14925,13266,14620,13807,14376,14323,13990,14545,13625,14713,13328,14840,12882,14931,12528,14996,12233,15039,11829,15066,11525,15080,11295,15085,10976,15082,10705,15073,10495,13880,14564,13898,14542,13977,14430,14158,14124,14393,13732,14556,13410,14702,12996,14814,12596,14891,12291,14937,11834,14957,11489,14958,11194,14943,10803,14921,10506,14893,10278,14858,9960,14484,14039,14487,14025,14499,13941,14524,13740,14574,13468,14654,13106,14743,12678,14818,12344,14867,11893,14889,11509,14893,11180,14881,10751,14852,10428,14812,10128,14765,9754,14712,9466,14764,13480,14764,13475,14766,13440,14766,13347,14769,13070,14786,12713,14816,12387,14844,11957,14860,11549,14868,11215,14855,10751,14825,10403,14782,10044,14729,9651,14666,9352,14599,9029,14967,12835,14966,12831,14963,12804,14954,12723,14936,12564,14917,12347,14900,11958,14886,11569,14878,11247,14859,10765,14828,10401,14784,10011,14727,9600,14660,9289,14586,8893,14508,8533,15111,12234,15110,12234,15104,12216,15092,12156,15067,12010,15028,11776,14981,11500,14942,11205,14902,10752,14861,10393,14812,9991,14752,9570,14682,9252,14603,8808,14519,8445,14431,8145,15209,11449,15208,11451,15202,11451,15190,11438,15163,11384,15117,11274,15055,10979,14994,10648,14932,10343,14871,9936,14803,9532,14729,9218,14645,8742,14556,8381,14461,8020,14365,7603,15273,10603,15272,10607,15267,10619,15256,10631,15231,10614,15182,10535,15118,10389,15042,10167,14963,9787,14883,9447,14800,9115,14710,8665,14615,8318,14514,7911,14411,7507,14279,7198,15314,9675,15313,9683,15309,9712,15298,9759,15277,9797,15229,9773,15166,9668,15084,9487,14995,9274,14898,8910,14800,8539,14697,8234,14590,7790,14479,7409,14367,7067,14178,6621,15337,8619,15337,8631,15333,8677,15325,8769,15305,8871,15264,8940,15202,8909,15119,8775,15022,8565,14916,8328,14804,8009,14688,7614,14569,7287,14448,6888,14321,6483,14088,6171,15350,7402,15350,7419,15347,7480,15340,7613,15322,7804,15287,7973,15229,8057,15148,8012,15046,7846,14933,7611,14810,7357,14682,7069,14552,6656,14421,6316,14251,5948,14007,5528,15356,5942,15356,5977,15353,6119,15348,6294,15332,6551,15302,6824,15249,7044,15171,7122,15070,7050,14949,6861,14818,6611,14679,6349,14538,6067,14398,5651,14189,5311,13935,4958,15359,4123,15359,4153,15356,4296,15353,4646,15338,5160,15311,5508,15263,5829,15188,6042,15088,6094,14966,6001,14826,5796,14678,5543,14527,5287,14377,4985,14133,4586,13869,4257,15360,1563,15360,1642,15358,2076,15354,2636,15341,3350,15317,4019,15273,4429,15203,4732,15105,4911,14981,4932,14836,4818,14679,4621,14517,4386,14359,4156,14083,3795,13808,3437,15360,122,15360,137,15358,285,15355,636,15344,1274,15322,2177,15281,2765,15215,3223,15120,3451,14995,3569,14846,3567,14681,3466,14511,3305,14344,3121,14037,2800,13753,2467,15360,0,15360,1,15359,21,15355,89,15346,253,15325,479,15287,796,15225,1148,15133,1492,15008,1749,14856,1882,14685,1886,14506,1783,14324,1608,13996,1398,13702,1183]);let qi=null;function TR(){return qi===null&&(qi=new Kb(ER,16,16,$s,Da),qi.name="DFG_LUT",qi.minFilter=Sn,qi.magFilter=Sn,qi.wrapS=wa,qi.wrapT=wa,qi.generateMipmaps=!1,qi.needsUpdate=!0),qi}class AR{constructor(t={}){const{canvas:n=fb(),context:a=null,depth:l=!0,stencil:c=!1,alpha:f=!1,antialias:d=!1,premultipliedAlpha:m=!0,preserveDrawingBuffer:h=!1,powerPreference:g="default",failIfMajorPerformanceCaveat:_=!1,reversedDepthBuffer:v=!1,outputBufferType:y=pi}=t;this.isWebGLRenderer=!0;let E;if(a!==null){if(typeof WebGLRenderingContext<"u"&&a instanceof WebGLRenderingContext)throw new Error("THREE.WebGLRenderer: WebGL 1 is not supported since r163.");E=a.getContextAttributes().alpha}else E=f;const A=y,S=new Set([Vp,Gp,Hp]),x=new Set([pi,ea,Ml,bl,Bp,Fp]),w=new Uint32Array(4),D=new Int32Array(4),U=new k;let G=null,O=null;const B=[],R=[];let z=null;this.domElement=n,this.debug={checkShaderErrors:!0,onShaderError:null},this.autoClear=!0,this.autoClearColor=!0,this.autoClearDepth=!0,this.autoClearStencil=!0,this.sortObjects=!0,this.clippingPlanes=[],this.localClippingEnabled=!1,this.toneMapping=$i,this.toneMappingExposure=1,this.transmissionResolutionScale=1;const K=this;let V=!1,$=null;this._outputColorSpace=Ti;let ht=0,gt=0,q=null,P=-1,F=null;const ct=new fn,J=new fn;let xt=null;const I=new _e(0);let Q=0,Mt=n.width,Rt=n.height,wt=1,st=null,bt=null;const Tt=new fn(0,0,Mt,Rt),Wt=new fn(0,0,Mt,Rt);let re=!1;const ie=new Zp;let Nt=!1,Ht=!1;const Ut=new tn,Gt=new k,ne=new fn,Me={background:null,fog:null,environment:null,overrideMaterial:null,isScene:!0};let le=!1;function Ue(){return q===null?wt:1}let W=a;function We(C,Y){return n.getContext(C,Y)}try{const C={alpha:!0,depth:l,stencil:c,antialias:d,premultipliedAlpha:m,preserveDrawingBuffer:h,powerPreference:g,failIfMajorPerformanceCaveat:_};if("setAttribute"in n&&n.setAttribute("data-engine",`three.js r${Ip}`),n.addEventListener("webglcontextlost",Et,!1),n.addEventListener("webglcontextrestored",Jt,!1),n.addEventListener("webglcontextcreationerror",ue,!1),W===null){const Y="webgl2";if(W=We(Y,C),W===null)throw We(Y)?new Error("Error creating WebGL context with your selected attributes."):new Error("Error creating WebGL context.")}}catch(C){throw Ne("WebGLRenderer: "+C.message),C}let ye,qe,Dt,an,L,T,tt,yt,At,Lt,zt,dt,pt,Bt,Ft,Pt,Ot,fe,de,be,j,Ct,_t;function jt(){ye=new TA(W),ye.init(),j=new gR(W,ye),qe=new _A(W,ye,t,j),Dt=new pR(W,ye),qe.reversedDepthBuffer&&v&&Dt.buffers.depth.setReversed(!0),an=new RA(W),L=new tR,T=new mR(W,ye,Dt,L,qe,j,an),tt=new EA(K),yt=new UE(W),Ct=new mA(W,yt),At=new AA(W,yt,an,Ct),Lt=new NA(W,At,yt,Ct,an),fe=new CA(W,qe,T),Ft=new vA(L),zt=new $w(K,tt,ye,qe,Ct,Ft),dt=new MR(K,L),pt=new nR,Bt=new lR(ye),Ot=new pA(K,tt,Dt,Lt,E,m),Pt=new hR(K,Lt,qe),_t=new bR(W,an,qe,Dt),de=new gA(W,ye,an),be=new wA(W,ye,an),an.programs=zt.programs,K.capabilities=qe,K.extensions=ye,K.properties=L,K.renderLists=pt,K.shadowMap=Pt,K.state=Dt,K.info=an}jt(),A!==pi&&(z=new UA(A,n.width,n.height,l,c));const It=new yR(K,W);this.xr=It,this.getContext=function(){return W},this.getContextAttributes=function(){return W.getContextAttributes()},this.forceContextLoss=function(){const C=ye.get("WEBGL_lose_context");C&&C.loseContext()},this.forceContextRestore=function(){const C=ye.get("WEBGL_lose_context");C&&C.restoreContext()},this.getPixelRatio=function(){return wt},this.setPixelRatio=function(C){C!==void 0&&(wt=C,this.setSize(Mt,Rt,!1))},this.getSize=function(C){return C.set(Mt,Rt)},this.setSize=function(C,Y,ot=!0){if(It.isPresenting){ce("WebGLRenderer: Can't change size while VR device is presenting.");return}Mt=C,Rt=Y,n.width=Math.floor(C*wt),n.height=Math.floor(Y*wt),ot===!0&&(n.style.width=C+"px",n.style.height=Y+"px"),z!==null&&z.setSize(n.width,n.height),this.setViewport(0,0,C,Y)},this.getDrawingBufferSize=function(C){return C.set(Mt*wt,Rt*wt).floor()},this.setDrawingBufferSize=function(C,Y,ot){Mt=C,Rt=Y,wt=ot,n.width=Math.floor(C*ot),n.height=Math.floor(Y*ot),this.setViewport(0,0,C,Y)},this.setEffects=function(C){if(A===pi){Ne("THREE.WebGLRenderer: setEffects() requires outputBufferType set to HalfFloatType or FloatType.");return}if(C){for(let Y=0;Y<C.length;Y++)if(C[Y].isOutputPass===!0){ce("THREE.WebGLRenderer: OutputPass is not needed in setEffects(). Tone mapping and color space conversion are applied automatically.");break}}z.setEffects(C||[])},this.getCurrentViewport=function(C){return C.copy(ct)},this.getViewport=function(C){return C.copy(Tt)},this.setViewport=function(C,Y,ot,it){C.isVector4?Tt.set(C.x,C.y,C.z,C.w):Tt.set(C,Y,ot,it),Dt.viewport(ct.copy(Tt).multiplyScalar(wt).round())},this.getScissor=function(C){return C.copy(Wt)},this.setScissor=function(C,Y,ot,it){C.isVector4?Wt.set(C.x,C.y,C.z,C.w):Wt.set(C,Y,ot,it),Dt.scissor(J.copy(Wt).multiplyScalar(wt).round())},this.getScissorTest=function(){return re},this.setScissorTest=function(C){Dt.setScissorTest(re=C)},this.setOpaqueSort=function(C){st=C},this.setTransparentSort=function(C){bt=C},this.getClearColor=function(C){return C.copy(Ot.getClearColor())},this.setClearColor=function(){Ot.setClearColor(...arguments)},this.getClearAlpha=function(){return Ot.getClearAlpha()},this.setClearAlpha=function(){Ot.setClearAlpha(...arguments)},this.clear=function(C=!0,Y=!0,ot=!0){let it=0;if(C){let at=!1;if(q!==null){const kt=q.texture.format;at=S.has(kt)}if(at){const kt=q.texture.type,Yt=x.has(kt),Vt=Ot.getClearColor(),Kt=Ot.getClearAlpha(),Zt=Vt.r,ae=Vt.g,me=Vt.b;Yt?(w[0]=Zt,w[1]=ae,w[2]=me,w[3]=Kt,W.clearBufferuiv(W.COLOR,0,w)):(D[0]=Zt,D[1]=ae,D[2]=me,D[3]=Kt,W.clearBufferiv(W.COLOR,0,D))}else it|=W.COLOR_BUFFER_BIT}Y&&(it|=W.DEPTH_BUFFER_BIT,this.state.buffers.depth.setMask(!0)),ot&&(it|=W.STENCIL_BUFFER_BIT,this.state.buffers.stencil.setMask(4294967295)),it!==0&&W.clear(it)},this.clearColor=function(){this.clear(!0,!1,!1)},this.clearDepth=function(){this.clear(!1,!0,!1)},this.clearStencil=function(){this.clear(!1,!1,!0)},this.setNodesHandler=function(C){C.setRenderer(this),$=C},this.dispose=function(){n.removeEventListener("webglcontextlost",Et,!1),n.removeEventListener("webglcontextrestored",Jt,!1),n.removeEventListener("webglcontextcreationerror",ue,!1),Ot.dispose(),pt.dispose(),Bt.dispose(),L.dispose(),tt.dispose(),Lt.dispose(),Ct.dispose(),_t.dispose(),zt.dispose(),It.dispose(),It.removeEventListener("sessionstart",fo),It.removeEventListener("sessionend",ho),In.stop()};function Et(C){C.preventDefault(),wu("WebGLRenderer: Context Lost."),V=!0}function Jt(){wu("WebGLRenderer: Context Restored."),V=!1;const C=an.autoReset,Y=Pt.enabled,ot=Pt.autoUpdate,it=Pt.needsUpdate,at=Pt.type;jt(),an.autoReset=C,Pt.enabled=Y,Pt.autoUpdate=ot,Pt.needsUpdate=it,Pt.type=at}function ue(C){Ne("WebGLRenderer: A WebGL context could not be created. Reason: ",C.statusMessage)}function ln(C){const Y=C.target;Y.removeEventListener("dispose",ln),Ie(Y)}function Ie(C){mi(C),L.remove(C)}function mi(C){const Y=L.get(C).programs;Y!==void 0&&(Y.forEach(function(ot){zt.releaseProgram(ot)}),C.isShaderMaterial&&zt.releaseShaderCache(C))}this.renderBufferDirect=function(C,Y,ot,it,at,kt){Y===null&&(Y=Me);const Yt=at.isMesh&&at.matrixWorld.determinant()<0,Vt=Ia(C,Y,ot,it,at);Dt.setMaterial(it,Yt);let Kt=ot.index,Zt=1;if(it.wireframe===!0){if(Kt=At.getWireframeAttribute(ot),Kt===void 0)return;Zt=2}const ae=ot.drawRange,me=ot.attributes.position;let te=ae.start*Zt,Le=(ae.start+ae.count)*Zt;kt!==null&&(te=Math.max(te,kt.start*Zt),Le=Math.min(Le,(kt.start+kt.count)*Zt)),Kt!==null?(te=Math.max(te,0),Le=Math.min(Le,Kt.count)):me!=null&&(te=Math.max(te,0),Le=Math.min(Le,me.count));const sn=Le-te;if(sn<0||sn===1/0)return;Ct.setup(at,it,Vt,ot,Kt);let Qe,Fe=de;if(Kt!==null&&(Qe=yt.get(Kt),Fe=be,Fe.setIndex(Qe)),at.isMesh)it.wireframe===!0?(Dt.setLineWidth(it.wireframeLinewidth*Ue()),Fe.setMode(W.LINES)):Fe.setMode(W.TRIANGLES);else if(at.isLine){let He=it.linewidth;He===void 0&&(He=1),Dt.setLineWidth(He*Ue()),at.isLineSegments?Fe.setMode(W.LINES):at.isLineLoop?Fe.setMode(W.LINE_LOOP):Fe.setMode(W.LINE_STRIP)}else at.isPoints?Fe.setMode(W.POINTS):at.isSprite&&Fe.setMode(W.TRIANGLES);if(at.isBatchedMesh)if(ye.get("WEBGL_multi_draw"))Fe.renderMultiDraw(at._multiDrawStarts,at._multiDrawCounts,at._multiDrawCount);else{const He=at._multiDrawStarts,qt=at._multiDrawCounts,zn=at._multiDrawCount,Ee=Kt?yt.get(Kt).bytesPerElement:1,Mn=L.get(it).currentProgram.getUniforms();for(let ni=0;ni<zn;ni++)Mn.setValue(W,"_gl_DrawID",ni),Fe.render(He[ni]/Ee,qt[ni])}else if(at.isInstancedMesh)Fe.renderInstances(te,sn,at.count);else if(ot.isInstancedBufferGeometry){const He=ot._maxInstanceCount!==void 0?ot._maxInstanceCount:1/0,qt=Math.min(ot.instanceCount,He);Fe.renderInstances(te,sn,qt)}else Fe.render(te,sn)};function ei(C,Y,ot){C.transparent===!0&&C.side===Aa&&C.forceSinglePass===!1?(C.side=ti,C.needsUpdate=!0,nr(C,Y,ot),C.side=vs,C.needsUpdate=!0,nr(C,Y,ot),C.side=Aa):nr(C,Y,ot)}this.compile=function(C,Y,ot=null){ot===null&&(ot=C),O=Bt.get(ot),O.init(Y),R.push(O),ot.traverseVisible(function(at){at.isLight&&at.layers.test(Y.layers)&&(O.pushLight(at),at.castShadow&&O.pushShadow(at))}),C!==ot&&C.traverseVisible(function(at){at.isLight&&at.layers.test(Y.layers)&&(O.pushLight(at),at.castShadow&&O.pushShadow(at))}),O.setupLights();const it=new Set;return C.traverse(function(at){if(!(at.isMesh||at.isPoints||at.isLine||at.isSprite))return;const kt=at.material;if(kt)if(Array.isArray(kt))for(let Yt=0;Yt<kt.length;Yt++){const Vt=kt[Yt];ei(Vt,ot,at),it.add(Vt)}else ei(kt,ot,at),it.add(kt)}),O=R.pop(),it},this.compileAsync=function(C,Y,ot=null){const it=this.compile(C,Y,ot);return new Promise(at=>{function kt(){if(it.forEach(function(Yt){L.get(Yt).currentProgram.isReady()&&it.delete(Yt)}),it.size===0){at(C);return}setTimeout(kt,10)}ye.get("KHR_parallel_shader_compile")!==null?kt():setTimeout(kt,10)})};let ys=null;function uo(C){ys&&ys(C)}function fo(){In.stop()}function ho(){In.start()}const In=new Kx;In.setAnimationLoop(uo),typeof self<"u"&&In.setContext(self),this.setAnimationLoop=function(C){ys=C,It.setAnimationLoop(C),C===null?In.stop():In.start()},It.addEventListener("sessionstart",fo),It.addEventListener("sessionend",ho),this.render=function(C,Y){if(Y!==void 0&&Y.isCamera!==!0){Ne("WebGLRenderer.render: camera is not an instance of THREE.Camera.");return}if(V===!0)return;$!==null&&$.renderStart(C,Y);const ot=It.enabled===!0&&It.isPresenting===!0,it=z!==null&&(q===null||ot)&&z.begin(K,q);if(C.matrixWorldAutoUpdate===!0&&C.updateMatrixWorld(),Y.parent===null&&Y.matrixWorldAutoUpdate===!0&&Y.updateMatrixWorld(),It.enabled===!0&&It.isPresenting===!0&&(z===null||z.isCompositing()===!1)&&(It.cameraAutoUpdate===!0&&It.updateCamera(Y),Y=It.getCamera()),C.isScene===!0&&C.onBeforeRender(K,C,Y,q),O=Bt.get(C,R.length),O.init(Y),O.state.textureUnits=T.getTextureUnits(),R.push(O),Ut.multiplyMatrices(Y.projectionMatrix,Y.matrixWorldInverse),ie.setFromProjectionMatrix(Ut,Ji,Y.reversedDepth),Ht=this.localClippingEnabled,Nt=Ft.init(this.clippingPlanes,Ht),G=pt.get(C,B.length),G.init(),B.push(G),It.enabled===!0&&It.isPresenting===!0){const Yt=K.xr.getDepthSensingMesh();Yt!==null&&dn(Yt,Y,-1/0,K.sortObjects)}dn(C,Y,0,K.sortObjects),G.finish(),K.sortObjects===!0&&G.sort(st,bt),le=It.enabled===!1||It.isPresenting===!1||It.hasDepthSensing()===!1,le&&Ot.addToRenderList(G,C),this.info.render.frame++,Nt===!0&&Ft.beginShadows();const at=O.state.shadowsArray;if(Pt.render(at,C,Y),Nt===!0&&Ft.endShadows(),this.info.autoReset===!0&&this.info.reset(),(it&&z.hasRenderPass())===!1){const Yt=G.opaque,Vt=G.transmissive;if(O.setupLights(),Y.isArrayCamera){const Kt=Y.cameras;if(Vt.length>0)for(let Zt=0,ae=Kt.length;Zt<ae;Zt++){const me=Kt[Zt];ia(Yt,Vt,C,me)}le&&Ot.render(C);for(let Zt=0,ae=Kt.length;Zt<ae;Zt++){const me=Kt[Zt];Cn(G,C,me,me.viewport)}}else Vt.length>0&&ia(Yt,Vt,C,Y),le&&Ot.render(C),Cn(G,C,Y)}q!==null&&gt===0&&(T.updateMultisampleRenderTarget(q),T.updateRenderTargetMipmap(q)),it&&z.end(K),C.isScene===!0&&C.onAfterRender(K,C,Y),Ct.resetDefaultState(),P=-1,F=null,R.pop(),R.length>0?(O=R[R.length-1],T.setTextureUnits(O.state.textureUnits),Nt===!0&&Ft.setGlobalState(K.clippingPlanes,O.state.camera)):O=null,B.pop(),B.length>0?G=B[B.length-1]:G=null,$!==null&&$.renderEnd()};function dn(C,Y,ot,it){if(C.visible===!1)return;if(C.layers.test(Y.layers)){if(C.isGroup)ot=C.renderOrder;else if(C.isLOD)C.autoUpdate===!0&&C.update(Y);else if(C.isLightProbeGrid)O.pushLightProbeGrid(C);else if(C.isLight)O.pushLight(C),C.castShadow&&O.pushShadow(C);else if(C.isSprite){if(!C.frustumCulled||ie.intersectsSprite(C)){it&&ne.setFromMatrixPosition(C.matrixWorld).applyMatrix4(Ut);const Yt=Lt.update(C),Vt=C.material;Vt.visible&&G.push(C,Yt,Vt,ot,ne.z,null)}}else if((C.isMesh||C.isLine||C.isPoints)&&(!C.frustumCulled||ie.intersectsObject(C))){const Yt=Lt.update(C),Vt=C.material;if(it&&(C.boundingSphere!==void 0?(C.boundingSphere===null&&C.computeBoundingSphere(),ne.copy(C.boundingSphere.center)):(Yt.boundingSphere===null&&Yt.computeBoundingSphere(),ne.copy(Yt.boundingSphere.center)),ne.applyMatrix4(C.matrixWorld).applyMatrix4(Ut)),Array.isArray(Vt)){const Kt=Yt.groups;for(let Zt=0,ae=Kt.length;Zt<ae;Zt++){const me=Kt[Zt],te=Vt[me.materialIndex];te&&te.visible&&G.push(C,Yt,te,ot,ne.z,me)}}else Vt.visible&&G.push(C,Yt,Vt,ot,ne.z,null)}}const kt=C.children;for(let Yt=0,Vt=kt.length;Yt<Vt;Yt++)dn(kt[Yt],Y,ot,it)}function Cn(C,Y,ot,it){const{opaque:at,transmissive:kt,transparent:Yt}=C;O.setupLightsView(ot),Nt===!0&&Ft.setGlobalState(K.clippingPlanes,ot),it&&Dt.viewport(ct.copy(it)),at.length>0&&Oa(at,Y,ot),kt.length>0&&Oa(kt,Y,ot),Yt.length>0&&Oa(Yt,Y,ot),Dt.buffers.depth.setTest(!0),Dt.buffers.depth.setMask(!0),Dt.buffers.color.setMask(!0),Dt.setPolygonOffset(!1)}function ia(C,Y,ot,it){if((ot.isScene===!0?ot.overrideMaterial:null)!==null)return;if(O.state.transmissionRenderTarget[it.id]===void 0){const te=ye.has("EXT_color_buffer_half_float")||ye.has("EXT_color_buffer_float");O.state.transmissionRenderTarget[it.id]=new ta(1,1,{generateMipmaps:!0,type:te?Da:pi,minFilter:Zs,samples:Math.max(4,qe.samples),stencilBuffer:c,resolveDepthBuffer:!1,resolveStencilBuffer:!1,colorSpace:De.workingColorSpace})}const kt=O.state.transmissionRenderTarget[it.id],Yt=it.viewport||ct;kt.setSize(Yt.z*K.transmissionResolutionScale,Yt.w*K.transmissionResolutionScale);const Vt=K.getRenderTarget(),Kt=K.getActiveCubeFace(),Zt=K.getActiveMipmapLevel();K.setRenderTarget(kt),K.getClearColor(I),Q=K.getClearAlpha(),Q<1&&K.setClearColor(16777215,.5),K.clear(),le&&Ot.render(ot);const ae=K.toneMapping;K.toneMapping=$i;const me=it.viewport;if(it.viewport!==void 0&&(it.viewport=void 0),O.setupLightsView(it),Nt===!0&&Ft.setGlobalState(K.clippingPlanes,it),Oa(C,ot,it),T.updateMultisampleRenderTarget(kt),T.updateRenderTargetMipmap(kt),ye.has("WEBGL_multisampled_render_to_texture")===!1){let te=!1;for(let Le=0,sn=Y.length;Le<sn;Le++){const Qe=Y[Le],{object:Fe,geometry:He,material:qt,group:zn}=Qe;if(qt.side===Aa&&Fe.layers.test(it.layers)){const Ee=qt.side;qt.side=ti,qt.needsUpdate=!0,Cl(Fe,ot,it,He,qt,zn),qt.side=Ee,qt.needsUpdate=!0,te=!0}}te===!0&&(T.updateMultisampleRenderTarget(kt),T.updateRenderTargetMipmap(kt))}K.setRenderTarget(Vt,Kt,Zt),K.setClearColor(I,Q),me!==void 0&&(it.viewport=me),K.toneMapping=ae}function Oa(C,Y,ot){const it=Y.isScene===!0?Y.overrideMaterial:null;for(let at=0,kt=C.length;at<kt;at++){const Yt=C[at],{object:Vt,geometry:Kt,group:Zt}=Yt;let ae=Yt.material;ae.allowOverride===!0&&it!==null&&(ae=it),Vt.layers.test(ot.layers)&&Cl(Vt,Y,ot,Kt,ae,Zt)}}function Cl(C,Y,ot,it,at,kt){C.onBeforeRender(K,Y,ot,it,at,kt),C.modelViewMatrix.multiplyMatrices(ot.matrixWorldInverse,C.matrixWorld),C.normalMatrix.getNormalMatrix(C.modelViewMatrix),at.onBeforeRender(K,Y,ot,it,C,kt),at.transparent===!0&&at.side===Aa&&at.forceSinglePass===!1?(at.side=ti,at.needsUpdate=!0,K.renderBufferDirect(ot,Y,it,at,C,kt),at.side=vs,at.needsUpdate=!0,K.renderBufferDirect(ot,Y,it,at,C,kt),at.side=Aa):K.renderBufferDirect(ot,Y,it,at,C,kt),C.onAfterRender(K,Y,ot,it,at,kt)}function nr(C,Y,ot){Y.isScene!==!0&&(Y=Me);const it=L.get(C),at=O.state.lights,kt=O.state.shadowsArray,Yt=at.state.version,Vt=zt.getParameters(C,at.state,kt,Y,ot,O.state.lightProbeGridArray),Kt=zt.getProgramCacheKey(Vt);let Zt=it.programs;it.environment=C.isMeshStandardMaterial||C.isMeshLambertMaterial||C.isMeshPhongMaterial?Y.environment:null,it.fog=Y.fog;const ae=C.isMeshStandardMaterial||C.isMeshLambertMaterial&&!C.envMap||C.isMeshPhongMaterial&&!C.envMap;it.envMap=tt.get(C.envMap||it.environment,ae),it.envMapRotation=it.environment!==null&&C.envMap===null?Y.environmentRotation:C.envMapRotation,Zt===void 0&&(C.addEventListener("dispose",ln),Zt=new Map,it.programs=Zt);let me=Zt.get(Kt);if(me!==void 0){if(it.currentProgram===me&&it.lightsStateVersion===Yt)return Pa(C,Vt),me}else Vt.uniforms=zt.getUniforms(C),$!==null&&C.isNodeMaterial&&$.build(C,ot,Vt),C.onBeforeCompile(Vt,K),me=zt.acquireProgram(Vt,Kt),Zt.set(Kt,me),it.uniforms=Vt.uniforms;const te=it.uniforms;return(!C.isShaderMaterial&&!C.isRawShaderMaterial||C.clipping===!0)&&(te.clippingPlanes=Ft.uniform),Pa(C,Vt),it.needsLights=Ss(C),it.lightsStateVersion=Yt,it.needsLights&&(te.ambientLightColor.value=at.state.ambient,te.lightProbe.value=at.state.probe,te.directionalLights.value=at.state.directional,te.directionalLightShadows.value=at.state.directionalShadow,te.spotLights.value=at.state.spot,te.spotLightShadows.value=at.state.spotShadow,te.rectAreaLights.value=at.state.rectArea,te.ltc_1.value=at.state.rectAreaLTC1,te.ltc_2.value=at.state.rectAreaLTC2,te.pointLights.value=at.state.point,te.pointLightShadows.value=at.state.pointShadow,te.hemisphereLights.value=at.state.hemi,te.directionalShadowMatrix.value=at.state.directionalShadowMatrix,te.spotLightMatrix.value=at.state.spotLightMatrix,te.spotLightMap.value=at.state.spotLightMap,te.pointShadowMatrix.value=at.state.pointShadowMatrix),it.lightProbeGrid=O.state.lightProbeGridArray.length>0,it.currentProgram=me,it.uniformsList=null,me}function po(C){if(C.uniformsList===null){const Y=C.currentProgram.getUniforms();C.uniformsList=xu.seqWithValue(Y.seq,C.uniforms)}return C.uniformsList}function Pa(C,Y){const ot=L.get(C);ot.outputColorSpace=Y.outputColorSpace,ot.batching=Y.batching,ot.batchingColor=Y.batchingColor,ot.instancing=Y.instancing,ot.instancingColor=Y.instancingColor,ot.instancingMorph=Y.instancingMorph,ot.skinning=Y.skinning,ot.morphTargets=Y.morphTargets,ot.morphNormals=Y.morphNormals,ot.morphColors=Y.morphColors,ot.morphTargetsCount=Y.morphTargetsCount,ot.numClippingPlanes=Y.numClippingPlanes,ot.numIntersection=Y.numClipIntersection,ot.vertexAlphas=Y.vertexAlphas,ot.vertexTangents=Y.vertexTangents,ot.toneMapping=Y.toneMapping}function mo(C,Y){if(C.length===0)return null;if(C.length===1)return C[0].texture!==null?C[0]:null;U.setFromMatrixPosition(Y.matrixWorld);for(let ot=0,it=C.length;ot<it;ot++){const at=C[ot];if(at.texture!==null&&at.boundingBox.containsPoint(U))return at}return null}function Ia(C,Y,ot,it,at){Y.isScene!==!0&&(Y=Me),T.resetTextureUnits();const kt=Y.fog,Yt=it.isMeshStandardMaterial||it.isMeshLambertMaterial||it.isMeshPhongMaterial?Y.environment:null,Vt=q===null?K.outputColorSpace:q.isXRRenderTarget===!0?q.texture.colorSpace:De.workingColorSpace,Kt=it.isMeshStandardMaterial||it.isMeshLambertMaterial&&!it.envMap||it.isMeshPhongMaterial&&!it.envMap,Zt=tt.get(it.envMap||Yt,Kt),ae=it.vertexColors===!0&&!!ot.attributes.color&&ot.attributes.color.itemSize===4,me=!!ot.attributes.tangent&&(!!it.normalMap||it.anisotropy>0),te=!!ot.morphAttributes.position,Le=!!ot.morphAttributes.normal,sn=!!ot.morphAttributes.color;let Qe=$i;it.toneMapped&&(q===null||q.isXRRenderTarget===!0)&&(Qe=K.toneMapping);const Fe=ot.morphAttributes.position||ot.morphAttributes.normal||ot.morphAttributes.color,He=Fe!==void 0?Fe.length:0,qt=L.get(it),zn=O.state.lights;if(Nt===!0&&(Ht===!0||C!==F)){const Be=C===F&&it.id===P;Ft.setState(it,C,Be)}let Ee=!1;it.version===qt.__version?(qt.needsLights&&qt.lightsStateVersion!==zn.state.version||qt.outputColorSpace!==Vt||at.isBatchedMesh&&qt.batching===!1||!at.isBatchedMesh&&qt.batching===!0||at.isBatchedMesh&&qt.batchingColor===!0&&at.colorTexture===null||at.isBatchedMesh&&qt.batchingColor===!1&&at.colorTexture!==null||at.isInstancedMesh&&qt.instancing===!1||!at.isInstancedMesh&&qt.instancing===!0||at.isSkinnedMesh&&qt.skinning===!1||!at.isSkinnedMesh&&qt.skinning===!0||at.isInstancedMesh&&qt.instancingColor===!0&&at.instanceColor===null||at.isInstancedMesh&&qt.instancingColor===!1&&at.instanceColor!==null||at.isInstancedMesh&&qt.instancingMorph===!0&&at.morphTexture===null||at.isInstancedMesh&&qt.instancingMorph===!1&&at.morphTexture!==null||qt.envMap!==Zt||it.fog===!0&&qt.fog!==kt||qt.numClippingPlanes!==void 0&&(qt.numClippingPlanes!==Ft.numPlanes||qt.numIntersection!==Ft.numIntersection)||qt.vertexAlphas!==ae||qt.vertexTangents!==me||qt.morphTargets!==te||qt.morphNormals!==Le||qt.morphColors!==sn||qt.toneMapping!==Qe||qt.morphTargetsCount!==He||!!qt.lightProbeGrid!=O.state.lightProbeGridArray.length>0)&&(Ee=!0):(Ee=!0,qt.__version=it.version);let Mn=qt.currentProgram;Ee===!0&&(Mn=nr(it,Y,at),$&&it.isNodeMaterial&&$.onUpdateProgram(it,Mn,qt));let ni=!1,Di=!1,ii=!1;const Ge=Mn.getUniforms(),rn=qt.uniforms;if(Dt.useProgram(Mn.program)&&(ni=!0,Di=!0,ii=!0),it.id!==P&&(P=it.id,Di=!0),qt.needsLights){const Be=mo(O.state.lightProbeGridArray,at);qt.lightProbeGrid!==Be&&(qt.lightProbeGrid=Be,Di=!0)}if(ni||F!==C){Dt.buffers.depth.getReversed()&&C.reversedDepth!==!0&&(C._reversedDepth=!0,C.updateProjectionMatrix()),Ge.setValue(W,"projectionMatrix",C.projectionMatrix),Ge.setValue(W,"viewMatrix",C.matrixWorldInverse);const Gi=Ge.map.cameraPosition;Gi!==void 0&&Gi.setValue(W,Gt.setFromMatrixPosition(C.matrixWorld)),qe.logarithmicDepthBuffer&&Ge.setValue(W,"logDepthBufFC",2/(Math.log(C.far+1)/Math.LN2)),(it.isMeshPhongMaterial||it.isMeshToonMaterial||it.isMeshLambertMaterial||it.isMeshBasicMaterial||it.isMeshStandardMaterial||it.isShaderMaterial)&&Ge.setValue(W,"isOrthographic",C.isOrthographicCamera===!0),F!==C&&(F=C,Di=!0,ii=!0)}if(qt.needsLights&&(zn.state.directionalShadowMap.length>0&&Ge.setValue(W,"directionalShadowMap",zn.state.directionalShadowMap,T),zn.state.spotShadowMap.length>0&&Ge.setValue(W,"spotShadowMap",zn.state.spotShadowMap,T),zn.state.pointShadowMap.length>0&&Ge.setValue(W,"pointShadowMap",zn.state.pointShadowMap,T)),at.isSkinnedMesh){Ge.setOptional(W,at,"bindMatrix"),Ge.setOptional(W,at,"bindMatrixInverse");const Be=at.skeleton;Be&&(Be.boneTexture===null&&Be.computeBoneTexture(),Ge.setValue(W,"boneTexture",Be.boneTexture,T))}at.isBatchedMesh&&(Ge.setOptional(W,at,"batchingTexture"),Ge.setValue(W,"batchingTexture",at._matricesTexture,T),Ge.setOptional(W,at,"batchingIdTexture"),Ge.setValue(W,"batchingIdTexture",at._indirectTexture,T),Ge.setOptional(W,at,"batchingColorTexture"),at._colorsTexture!==null&&Ge.setValue(W,"batchingColorTexture",at._colorsTexture,T));const Ui=ot.morphAttributes;if((Ui.position!==void 0||Ui.normal!==void 0||Ui.color!==void 0)&&fe.update(at,ot,Mn),(Di||qt.receiveShadow!==at.receiveShadow)&&(qt.receiveShadow=at.receiveShadow,Ge.setValue(W,"receiveShadow",at.receiveShadow)),(it.isMeshStandardMaterial||it.isMeshLambertMaterial||it.isMeshPhongMaterial)&&it.envMap===null&&Y.environment!==null&&(rn.envMapIntensity.value=Y.environmentIntensity),rn.dfgLUT!==void 0&&(rn.dfgLUT.value=TR()),Di){if(Ge.setValue(W,"toneMappingExposure",K.toneMappingExposure),qt.needsLights&&za(rn,ii),kt&&it.fog===!0&&dt.refreshFogUniforms(rn,kt),dt.refreshMaterialUniforms(rn,it,wt,Rt,O.state.transmissionRenderTarget[C.id]),qt.needsLights&&qt.lightProbeGrid){const Be=qt.lightProbeGrid;rn.probesSH.value=Be.texture,rn.probesMin.value.copy(Be.boundingBox.min),rn.probesMax.value.copy(Be.boundingBox.max),rn.probesResolution.value.copy(Be.resolution)}xu.upload(W,po(qt),rn,T)}if(it.isShaderMaterial&&it.uniformsNeedUpdate===!0&&(xu.upload(W,po(qt),rn,T),it.uniformsNeedUpdate=!1),it.isSpriteMaterial&&Ge.setValue(W,"center",at.center),Ge.setValue(W,"modelViewMatrix",at.modelViewMatrix),Ge.setValue(W,"normalMatrix",at.normalMatrix),Ge.setValue(W,"modelMatrix",at.matrixWorld),it.uniformsGroups!==void 0){const Be=it.uniformsGroups;for(let Gi=0,Fa=Be.length;Gi<Fa;Gi++){const Ms=Be[Gi];_t.update(Ms,Mn),_t.bind(Ms,Mn)}}return Mn}function za(C,Y){C.ambientLightColor.needsUpdate=Y,C.lightProbe.needsUpdate=Y,C.directionalLights.needsUpdate=Y,C.directionalLightShadows.needsUpdate=Y,C.pointLights.needsUpdate=Y,C.pointLightShadows.needsUpdate=Y,C.spotLights.needsUpdate=Y,C.spotLightShadows.needsUpdate=Y,C.rectAreaLights.needsUpdate=Y,C.hemisphereLights.needsUpdate=Y}function Ss(C){return C.isMeshLambertMaterial||C.isMeshToonMaterial||C.isMeshPhongMaterial||C.isMeshStandardMaterial||C.isShadowMaterial||C.isShaderMaterial&&C.lights===!0}this.getActiveCubeFace=function(){return ht},this.getActiveMipmapLevel=function(){return gt},this.getRenderTarget=function(){return q},this.setRenderTargetTextures=function(C,Y,ot){const it=L.get(C);it.__autoAllocateDepthBuffer=C.resolveDepthBuffer===!1,it.__autoAllocateDepthBuffer===!1&&(it.__useRenderToTexture=!1),L.get(C.texture).__webglTexture=Y,L.get(C.depthTexture).__webglTexture=it.__autoAllocateDepthBuffer?void 0:ot,it.__hasExternalTextures=!0},this.setRenderTargetFramebuffer=function(C,Y){const ot=L.get(C);ot.__webglFramebuffer=Y,ot.__useDefaultFramebuffer=Y===void 0};const Ba=W.createFramebuffer();this.setRenderTarget=function(C,Y=0,ot=0){q=C,ht=Y,gt=ot;let it=null,at=!1,kt=!1;if(C){const Vt=L.get(C);if(Vt.__useDefaultFramebuffer!==void 0){Dt.bindFramebuffer(W.FRAMEBUFFER,Vt.__webglFramebuffer),ct.copy(C.viewport),J.copy(C.scissor),xt=C.scissorTest,Dt.viewport(ct),Dt.scissor(J),Dt.setScissorTest(xt),P=-1;return}else if(Vt.__webglFramebuffer===void 0)T.setupRenderTarget(C);else if(Vt.__hasExternalTextures)T.rebindTextures(C,L.get(C.texture).__webglTexture,L.get(C.depthTexture).__webglTexture);else if(C.depthBuffer){const ae=C.depthTexture;if(Vt.__boundDepthTexture!==ae){if(ae!==null&&L.has(ae)&&(C.width!==ae.image.width||C.height!==ae.image.height))throw new Error("WebGLRenderTarget: Attached DepthTexture is initialized to the incorrect size.");T.setupDepthRenderbuffer(C)}}const Kt=C.texture;(Kt.isData3DTexture||Kt.isDataArrayTexture||Kt.isCompressedArrayTexture)&&(kt=!0);const Zt=L.get(C).__webglFramebuffer;C.isWebGLCubeRenderTarget?(Array.isArray(Zt[Y])?it=Zt[Y][ot]:it=Zt[Y],at=!0):C.samples>0&&T.useMultisampledRTT(C)===!1?it=L.get(C).__webglMultisampledFramebuffer:Array.isArray(Zt)?it=Zt[ot]:it=Zt,ct.copy(C.viewport),J.copy(C.scissor),xt=C.scissorTest}else ct.copy(Tt).multiplyScalar(wt).floor(),J.copy(Wt).multiplyScalar(wt).floor(),xt=re;if(ot!==0&&(it=Ba),Dt.bindFramebuffer(W.FRAMEBUFFER,it)&&Dt.drawBuffers(C,it),Dt.viewport(ct),Dt.scissor(J),Dt.setScissorTest(xt),at){const Vt=L.get(C.texture);W.framebufferTexture2D(W.FRAMEBUFFER,W.COLOR_ATTACHMENT0,W.TEXTURE_CUBE_MAP_POSITIVE_X+Y,Vt.__webglTexture,ot)}else if(kt){const Vt=Y;for(let Kt=0;Kt<C.textures.length;Kt++){const Zt=L.get(C.textures[Kt]);W.framebufferTextureLayer(W.FRAMEBUFFER,W.COLOR_ATTACHMENT0+Kt,Zt.__webglTexture,ot,Vt)}}else if(C!==null&&ot!==0){const Vt=L.get(C.texture);W.framebufferTexture2D(W.FRAMEBUFFER,W.COLOR_ATTACHMENT0,W.TEXTURE_2D,Vt.__webglTexture,ot)}P=-1},this.readRenderTargetPixels=function(C,Y,ot,it,at,kt,Yt,Vt=0){if(!(C&&C.isWebGLRenderTarget)){Ne("WebGLRenderer.readRenderTargetPixels: renderTarget is not THREE.WebGLRenderTarget.");return}let Kt=L.get(C).__webglFramebuffer;if(C.isWebGLCubeRenderTarget&&Yt!==void 0&&(Kt=Kt[Yt]),Kt){Dt.bindFramebuffer(W.FRAMEBUFFER,Kt);try{const Zt=C.textures[Vt],ae=Zt.format,me=Zt.type;if(C.textures.length>1&&W.readBuffer(W.COLOR_ATTACHMENT0+Vt),!qe.textureFormatReadable(ae)){Ne("WebGLRenderer.readRenderTargetPixels: renderTarget is not in RGBA or implementation defined format.");return}if(!qe.textureTypeReadable(me)){Ne("WebGLRenderer.readRenderTargetPixels: renderTarget is not in UnsignedByteType or implementation defined type.");return}Y>=0&&Y<=C.width-it&&ot>=0&&ot<=C.height-at&&W.readPixels(Y,ot,it,at,j.convert(ae),j.convert(me),kt)}finally{const Zt=q!==null?L.get(q).__webglFramebuffer:null;Dt.bindFramebuffer(W.FRAMEBUFFER,Zt)}}},this.readRenderTargetPixelsAsync=async function(C,Y,ot,it,at,kt,Yt,Vt=0){if(!(C&&C.isWebGLRenderTarget))throw new Error("THREE.WebGLRenderer.readRenderTargetPixels: renderTarget is not THREE.WebGLRenderTarget.");let Kt=L.get(C).__webglFramebuffer;if(C.isWebGLCubeRenderTarget&&Yt!==void 0&&(Kt=Kt[Yt]),Kt)if(Y>=0&&Y<=C.width-it&&ot>=0&&ot<=C.height-at){Dt.bindFramebuffer(W.FRAMEBUFFER,Kt);const Zt=C.textures[Vt],ae=Zt.format,me=Zt.type;if(C.textures.length>1&&W.readBuffer(W.COLOR_ATTACHMENT0+Vt),!qe.textureFormatReadable(ae))throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: renderTarget is not in RGBA or implementation defined format.");if(!qe.textureTypeReadable(me))throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: renderTarget is not in UnsignedByteType or implementation defined type.");const te=W.createBuffer();W.bindBuffer(W.PIXEL_PACK_BUFFER,te),W.bufferData(W.PIXEL_PACK_BUFFER,kt.byteLength,W.STREAM_READ),W.readPixels(Y,ot,it,at,j.convert(ae),j.convert(me),0);const Le=q!==null?L.get(q).__webglFramebuffer:null;Dt.bindFramebuffer(W.FRAMEBUFFER,Le);const sn=W.fenceSync(W.SYNC_GPU_COMMANDS_COMPLETE,0);return W.flush(),await db(W,sn,4),W.bindBuffer(W.PIXEL_PACK_BUFFER,te),W.getBufferSubData(W.PIXEL_PACK_BUFFER,0,kt),W.deleteBuffer(te),W.deleteSync(sn),kt}else throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: requested read bounds are out of range.")},this.copyFramebufferToTexture=function(C,Y=null,ot=0){const it=Math.pow(2,-ot),at=Math.floor(C.image.width*it),kt=Math.floor(C.image.height*it),Yt=Y!==null?Y.x:0,Vt=Y!==null?Y.y:0;T.setTexture2D(C,0),W.copyTexSubImage2D(W.TEXTURE_2D,ot,0,0,Yt,Vt,at,kt),Dt.unbindTexture()};const mn=W.createFramebuffer(),Nl=W.createFramebuffer();this.copyTextureToTexture=function(C,Y,ot=null,it=null,at=0,kt=0){let Yt,Vt,Kt,Zt,ae,me,te,Le,sn;const Qe=C.isCompressedTexture?C.mipmaps[kt]:C.image;if(ot!==null)Yt=ot.max.x-ot.min.x,Vt=ot.max.y-ot.min.y,Kt=ot.isBox3?ot.max.z-ot.min.z:1,Zt=ot.min.x,ae=ot.min.y,me=ot.isBox3?ot.min.z:0;else{const rn=Math.pow(2,-at);Yt=Math.floor(Qe.width*rn),Vt=Math.floor(Qe.height*rn),C.isDataArrayTexture?Kt=Qe.depth:C.isData3DTexture?Kt=Math.floor(Qe.depth*rn):Kt=1,Zt=0,ae=0,me=0}it!==null?(te=it.x,Le=it.y,sn=it.z):(te=0,Le=0,sn=0);const Fe=j.convert(Y.format),He=j.convert(Y.type);let qt;Y.isData3DTexture?(T.setTexture3D(Y,0),qt=W.TEXTURE_3D):Y.isDataArrayTexture||Y.isCompressedArrayTexture?(T.setTexture2DArray(Y,0),qt=W.TEXTURE_2D_ARRAY):(T.setTexture2D(Y,0),qt=W.TEXTURE_2D),Dt.activeTexture(W.TEXTURE0),Dt.pixelStorei(W.UNPACK_FLIP_Y_WEBGL,Y.flipY),Dt.pixelStorei(W.UNPACK_PREMULTIPLY_ALPHA_WEBGL,Y.premultiplyAlpha),Dt.pixelStorei(W.UNPACK_ALIGNMENT,Y.unpackAlignment);const zn=Dt.getParameter(W.UNPACK_ROW_LENGTH),Ee=Dt.getParameter(W.UNPACK_IMAGE_HEIGHT),Mn=Dt.getParameter(W.UNPACK_SKIP_PIXELS),ni=Dt.getParameter(W.UNPACK_SKIP_ROWS),Di=Dt.getParameter(W.UNPACK_SKIP_IMAGES);Dt.pixelStorei(W.UNPACK_ROW_LENGTH,Qe.width),Dt.pixelStorei(W.UNPACK_IMAGE_HEIGHT,Qe.height),Dt.pixelStorei(W.UNPACK_SKIP_PIXELS,Zt),Dt.pixelStorei(W.UNPACK_SKIP_ROWS,ae),Dt.pixelStorei(W.UNPACK_SKIP_IMAGES,me);const ii=C.isDataArrayTexture||C.isData3DTexture,Ge=Y.isDataArrayTexture||Y.isData3DTexture;if(C.isDepthTexture){const rn=L.get(C),Ui=L.get(Y),Be=L.get(rn.__renderTarget),Gi=L.get(Ui.__renderTarget);Dt.bindFramebuffer(W.READ_FRAMEBUFFER,Be.__webglFramebuffer),Dt.bindFramebuffer(W.DRAW_FRAMEBUFFER,Gi.__webglFramebuffer);for(let Fa=0;Fa<Kt;Fa++)ii&&(W.framebufferTextureLayer(W.READ_FRAMEBUFFER,W.COLOR_ATTACHMENT0,L.get(C).__webglTexture,at,me+Fa),W.framebufferTextureLayer(W.DRAW_FRAMEBUFFER,W.COLOR_ATTACHMENT0,L.get(Y).__webglTexture,kt,sn+Fa)),W.blitFramebuffer(Zt,ae,Yt,Vt,te,Le,Yt,Vt,W.DEPTH_BUFFER_BIT,W.NEAREST);Dt.bindFramebuffer(W.READ_FRAMEBUFFER,null),Dt.bindFramebuffer(W.DRAW_FRAMEBUFFER,null)}else if(at!==0||C.isRenderTargetTexture||L.has(C)){const rn=L.get(C),Ui=L.get(Y);Dt.bindFramebuffer(W.READ_FRAMEBUFFER,mn),Dt.bindFramebuffer(W.DRAW_FRAMEBUFFER,Nl);for(let Be=0;Be<Kt;Be++)ii?W.framebufferTextureLayer(W.READ_FRAMEBUFFER,W.COLOR_ATTACHMENT0,rn.__webglTexture,at,me+Be):W.framebufferTexture2D(W.READ_FRAMEBUFFER,W.COLOR_ATTACHMENT0,W.TEXTURE_2D,rn.__webglTexture,at),Ge?W.framebufferTextureLayer(W.DRAW_FRAMEBUFFER,W.COLOR_ATTACHMENT0,Ui.__webglTexture,kt,sn+Be):W.framebufferTexture2D(W.DRAW_FRAMEBUFFER,W.COLOR_ATTACHMENT0,W.TEXTURE_2D,Ui.__webglTexture,kt),at!==0?W.blitFramebuffer(Zt,ae,Yt,Vt,te,Le,Yt,Vt,W.COLOR_BUFFER_BIT,W.NEAREST):Ge?W.copyTexSubImage3D(qt,kt,te,Le,sn+Be,Zt,ae,Yt,Vt):W.copyTexSubImage2D(qt,kt,te,Le,Zt,ae,Yt,Vt);Dt.bindFramebuffer(W.READ_FRAMEBUFFER,null),Dt.bindFramebuffer(W.DRAW_FRAMEBUFFER,null)}else Ge?C.isDataTexture||C.isData3DTexture?W.texSubImage3D(qt,kt,te,Le,sn,Yt,Vt,Kt,Fe,He,Qe.data):Y.isCompressedArrayTexture?W.compressedTexSubImage3D(qt,kt,te,Le,sn,Yt,Vt,Kt,Fe,Qe.data):W.texSubImage3D(qt,kt,te,Le,sn,Yt,Vt,Kt,Fe,He,Qe):C.isDataTexture?W.texSubImage2D(W.TEXTURE_2D,kt,te,Le,Yt,Vt,Fe,He,Qe.data):C.isCompressedTexture?W.compressedTexSubImage2D(W.TEXTURE_2D,kt,te,Le,Qe.width,Qe.height,Fe,Qe.data):W.texSubImage2D(W.TEXTURE_2D,kt,te,Le,Yt,Vt,Fe,He,Qe);Dt.pixelStorei(W.UNPACK_ROW_LENGTH,zn),Dt.pixelStorei(W.UNPACK_IMAGE_HEIGHT,Ee),Dt.pixelStorei(W.UNPACK_SKIP_PIXELS,Mn),Dt.pixelStorei(W.UNPACK_SKIP_ROWS,ni),Dt.pixelStorei(W.UNPACK_SKIP_IMAGES,Di),kt===0&&Y.generateMipmaps&&W.generateMipmap(qt),Dt.unbindTexture()},this.initRenderTarget=function(C){L.get(C).__webglFramebuffer===void 0&&T.setupRenderTarget(C)},this.initTexture=function(C){C.isCubeTexture?T.setTextureCube(C,0):C.isData3DTexture?T.setTexture3D(C,0):C.isDataArrayTexture||C.isCompressedArrayTexture?T.setTexture2DArray(C,0):T.setTexture2D(C,0),Dt.unbindTexture()},this.resetState=function(){ht=0,gt=0,q=null,Dt.reset(),Ct.reset()},typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("observe",{detail:this}))}get coordinateSystem(){return Ji}get outputColorSpace(){return this._outputColorSpace}set outputColorSpace(t){this._outputColorSpace=t;const n=this.getContext();n.drawingBufferColorSpace=De._getDrawingBufferColorSpace(t),n.unpackColorSpace=De._getUnpackColorSpace()}}const Lh=r=>{const t=r.offering||r.name||"";return r.instance_name?`${t}:${r.instance_name}`:t},wR=r=>r>85?"#c45050":r>70?"#d4a373":"#84a59d",yu=r=>r==="thriving"||r==="healthy"?"#84a59d":r==="withering"||r==="degraded"?"#d4a373":r==="unhealthy"?"#c45050":"#78716c",ay=r=>r!=="resting"&&r!=="installing",du=document.createElement("canvas").getContext("2d");function Nu(r,t){du.clearRect(0,0,1,1),du.fillStyle=r,du.fillRect(0,0,1,1);const[n,a,l]=du.getImageData(0,0,1,1).data;return`rgba(${n},${a},${l},${t})`}const Np="#84a59d";function RR(r){const t=r.stone_name||"";return t.startsWith("stone-")?t.slice(6):t}function CR(r){return r.resources?.cpu_cores??0}function NR(r){const t=r.resources?.memory_total_bytes??0;return Math.round(t/1073741824)}function DR(r){return{cpu:r.resources?.cpu_percent??0,mem:r.resources?.memory_percent??0,dsk:r.resources?.disk_percent??0}}function $r(r){if(r===0)return[];if(r===1)return[[0,0,1]];const t=[],n=Math.PI*(3-Math.sqrt(5));for(let a=0;a<r;a++){const l=1-a/(r-1)*2,c=Math.sqrt(1-l*l),f=n*a;t.push([Math.cos(f)*c,l,Math.sin(f)*c])}return t}function UR(r,t,n,a=48){const l=r.clone().normalize(),c=t.clone().normalize(),f=Math.acos(Zi.clamp(l.dot(c),-1,1)),d=Math.sin(f),m=[];for(let h=0;h<=a;h++){const g=h/a;let _;if(d<.001)_=l.clone().lerp(c,g).normalize();else{const v=Math.sin((1-g)*f)/d,y=Math.sin(g*f)/d;_=new k(l.x*v+c.x*y,l.y*v+c.y*y,l.z*v+c.z*y).normalize()}m.push(_.multiplyScalar(n*1.003))}return m}function LR(r){const t=[];for(let n=0;n<r.length;n++)for(let a=n+1;a<r.length;a++){const l=new Set;let c=0;const f=r[n].offerings||[],d=r[a].offerings||[];f.forEach(m=>d.forEach(h=>{if(Lh(m)===Lh(h)&&(l.add(Lh(m)),c===0)){const g=(m.role||"").toLowerCase(),_=(h.role||"").toLowerCase();g==="primary"&&(_==="replica"||_==="joining")?c=1:_==="primary"&&(g==="replica"||g==="joining")&&(c=2)}})),l.size>0&&t.push({from:n,to:a,sets:[...l],direction:c})}return t}function ex(r,t=!1){const m=document.createElement("canvas");m.width=512,m.height=512;const h=m.getContext("2d"),g=Math.PI*2/3-.14,_=ay(r.health)&&!t,v=DR(r),y=r.color||Np;[v.cpu,v.mem,v.dsk].forEach((O,B)=>{const R=B*(Math.PI*2)/3-Math.PI/2+.07;if(h.beginPath(),h.arc(256,195,148,R,R+g),h.strokeStyle="rgba(255,255,255,0.07)",h.lineWidth=7,h.lineCap="round",h.stroke(),_){const z=g*(O/100);z>.02&&(h.beginPath(),h.arc(256,195,148,R,R+z),h.strokeStyle=t?"#555":wR(O),h.lineWidth=7,h.lineCap="round",h.stroke())}}),h.beginPath(),h.arc(256,195,126,0,Math.PI*2),h.strokeStyle=Nu(t?"#555":y,_?.21:.09),h.lineWidth=2,h.stroke();const E=t?"#555":yu(r.health);h.shadowColor=E,h.shadowBlur=_?25:8,h.beginPath(),h.arc(256,195,_?8:5,0,Math.PI*2),h.fillStyle=E,h.fill(),h.shadowBlur=0;const A=RR(r);h.font=`500 ${_?28:24}px "IBM Plex Sans",sans-serif`,h.fillStyle=t?"#555":_?"#fafaf9":"#78716c",h.textAlign="center",h.textBaseline="top",h.fillText(A,256,355);const S=CR(r),x=NR(r);t?(h.font='500 20px "IBM Plex Mono",monospace',h.fillStyle="#555",h.fillText("OFFLINE",256,389)):(h.font='300 17px "IBM Plex Mono",monospace',h.fillStyle="#78716c",h.fillText(`${S}c · ${x}GB`,256,389));const w=r.offerings||[],D=13,U=256-(w.length-1)*D/2;w.forEach((O,B)=>{h.beginPath(),h.arc(U+B*D,415,4,0,Math.PI*2),!t&&O.status==="running"?(h.fillStyle="#84a59d",h.fill()):(h.strokeStyle="#57534e",h.lineWidth=1.5,h.stroke())});const G=r.tags?.includes("keystone")||r._pond==="keystone";return!t&&G&&(h.font='400 13px "IBM Plex Mono",monospace',h.fillStyle="#c4b060",h.fillText("◆ keystone",256,436)),m}function Yi(r){return r.id||r.replica_set_id||r.replica_set_name||r.name||""}function nx(r){const f=document.createElement("canvas");f.width=384,f.height=384;const d=f.getContext("2d"),m=OR(r),h=(r.local_volume_count||0)>0&&(r.replica_count||r.replicas?.length||0)>1,g="#c4b060",_=-Math.PI/2;d.beginPath(),d.arc(192,160,110,_,_+Math.PI*2),d.strokeStyle="rgba(255,255,255,0.07)",d.lineWidth=6,d.stroke(),m>.5&&(d.beginPath(),d.arc(192,160,110,_,_+Math.PI*2*(m/100)),d.strokeStyle=m>85?"#c45050":m>70?"#d4a373":g,d.lineWidth=6,d.lineCap="round",d.stroke()),d.beginPath(),d.arc(192,160,94,0,Math.PI*2),d.strokeStyle=Nu(g,h?.45:.22),d.lineWidth=h?3:2,d.stroke(),d.shadowColor=g,d.shadowBlur=14,d.fillStyle=g,d.translate(192,160),d.rotate(Math.PI/4),d.fillRect(-7,-7,14,14),d.rotate(-Math.PI/4),d.translate(-192,-160),d.shadowBlur=0,d.font='500 24px "IBM Plex Sans",sans-serif',d.fillStyle="#fafaf9",d.textAlign="center",d.textBaseline="top",d.fillText(r.name||r.replica_set_name||"bank",192,282),d.font='300 14px "IBM Plex Mono",monospace',d.fillStyle="#78716c";const v=ix(r.capacity_bytes||0),y=ix(r.used_bytes||0);return d.fillText(`${y} / ${v}`,192,312),r._seedCount&&r._seedCount>0&&(d.font='500 12px "IBM Plex Mono",monospace',d.fillStyle=g,d.fillText(`${r._seedCount} seed${r._seedCount===1?"":"s"}`,192,338)),f}function OR(r){const t=r.capacity_bytes||0;if(t<=0)return 0;const n=r.used_bytes||0;return Math.min(100,n/t*100)}function ix(r){if(!r)return"—";const t=["B","K","M","G","T","P"];let n=0,a=r;for(;a>=1024&&n<t.length-1;)a/=1024,n+=1;return`${a.toFixed(a<10&&n>0?1:0)}${t[n]}`}function ax(r="#84a59d"){const t=document.createElement("canvas");t.width=128,t.height=128;const n=t.getContext("2d"),a=n.createRadialGradient(64,64,0,64,64,64);return a.addColorStop(0,Nu(r,.38)),a.addColorStop(.4,Nu(r,.13)),a.addColorStop(1,"transparent"),n.fillStyle=a,n.fillRect(0,0,128,128),new qs(t)}function PR(){const r=document.createElement("canvas");r.width=32,r.height=32;const t=r.getContext("2d"),n=t.createRadialGradient(16,16,0,16,16,16);return n.addColorStop(0,"#ffffff"),n.addColorStop(.3,"#84a59dcc"),n.addColorStop(1,"transparent"),t.fillStyle=n,t.fillRect(0,0,32,32),new qs(r)}class IR{constructor(t,n={}){this.container=t,this.R=n.radius||10,this.onHover=n.onHover||(()=>{}),this.onTrack=n.onTrack||(()=>{}),this.onTransition=n.onTransition||(()=>{}),this.onDataChange=n.onDataChange||(()=>{}),this.nodes=[],this.conns=[],this.hitTargets=[],this.stones=[],this.banks=[],this.bankNodes=[],this.bankHitTargets=[],this.bankRadius=(n.radius||10)*.55,this.hoveredKind=null,this.hoveredId=null,this.selectedId=null,this.departingId=null,this.isDrag=!1,this.prevM={x:0,y:0},this.vel={x:0,y:0},this.lastInput=0,this.t0=performance.now(),this.mouseInCanvas=!1,this.autoRotMul=1,this.rotTarget=null,this.rotFrom=null,this.rotProgress=1,this.rotDuration=.9,this.layoutProgress=1;const a=t.clientWidth,l=t.clientHeight;this.scene=new kb,this.camera=new hi(48,a/l,.1,200),this.camera.position.set(0,2,28),this.camera.lookAt(0,0,0),this.renderer=new AR({antialias:!0,alpha:!0}),this.renderer.setPixelRatio(Math.min(window.devicePixelRatio,2)),this.renderer.setSize(a,l),this.renderer.setClearColor(1118480,1),t.appendChild(this.renderer.domElement),this.scene.add(new wE(6316128,.6)),this.pLight=new AE(16777215,.7,60),this.pLight.position.copy(this.camera.position),this.scene.add(this.pLight),this.sg=new Qs,this.scene.add(this.sg),this.ringMat=new Ws({color:8693149,transparent:!0,opacity:.1});const c=new Gn(new Cu(this.R,.02,6,160),this.ringMat);c.rotation.x=-Math.PI/2,this.sg.add(c);const f=new Gn(new Cu(this.R,.012,6,160),new Ws({color:8693149,transparent:!0,opacity:.04}));f.rotation.y=Math.PI/3,this.sg.add(f);const d=250,m=new Float32Array(d*3);for(let g=0;g<d;g++){const _=25+Math.random()*30,v=Math.random()*Math.PI*2,y=Math.acos(2*Math.random()-1);m[g*3]=_*Math.sin(y)*Math.cos(v),m[g*3+1]=_*Math.sin(y)*Math.sin(v),m[g*3+2]=_*Math.cos(y)}const h=new qn;h.setAttribute("position",new Ci(m,3)),this.scene.add(new tE(h,new Fx({color:8693149,size:.06,transparent:!0,opacity:.15,sizeAttenuation:!0}))),this.ray=new NE,this.mouse=new ee,this.sparkTex=PR(),this._bindEvents(this.renderer.domElement),this._startAnim()}setData(t){this._clearAll(),this.stones=[...t];const n=$r(this.stones.length);this.stones.forEach((a,l)=>{const c=new k(...n[l]).multiplyScalar(this.R);this.nodes.push(this._mkNode(a,c))}),this._rebuildEdges(),this.onDataChange(this.stones)}updateStone(t,n){const a=this.nodes.find(c=>c.stone.stone_id===t);if(!a)return;const l=this.stones.findIndex(c=>c.stone_id===t);l>=0&&(this.stones[l]={...this.stones[l],...n}),Object.assign(a.stone,n),this._refreshTex(a),a.bodyMat.emissive=new _e(yu(a.stone.health)),n.offerings&&this._rebuildEdges(),this.onDataChange(this.stones)}addStone(t){this.stones.push(t);const n=$r(this.stones.length);this.nodes.forEach((c,f)=>{const[d,m,h]=n[f];c.targetPos=new k(d,m,h).multiplyScalar(this.R)});const a=new k(...n[this.stones.length-1]).multiplyScalar(this.R),l=this._mkNode(t,a);l.enterScale=0,this.nodes.push(l),this.layoutProgress=0,this._rebuildEdges(),this.onDataChange(this.stones)}removeStone(t){const n=this.nodes.findIndex(l=>l.stone.stone_id===t);if(n<0)return;const a=this.nodes[n];a.removing=!0,a.removeProgress=0,a.removeCallback=()=>{this.sg.remove(a.group),this.hitTargets=this.hitTargets.filter(c=>c.userData.stoneId!==t),this.nodes.splice(this.nodes.indexOf(a),1),this.stones=this.stones.filter(c=>c.stone_id!==t),this.selectedId===t&&(this.selectedId=null,this.onTransition({selectedId:null,departingId:t})),this.hoveredId===t&&(this.hoveredId=null);const l=$r(this.stones.length);this.nodes.forEach((c,f)=>{c.targetPos=new k(...l[f]).multiplyScalar(this.R)}),this.layoutProgress=0,this._rebuildEdges(),this.onDataChange(this.stones)}}offlineStone(t){const n=this.nodes.find(a=>a.stone.stone_id===t);n&&(n.offline=!0,this._refreshTex(n),n.bodyMat.color=new _e("#444"),n.bodyMat.emissive=new _e("#333"),this._rebuildEdges(),this.onDataChange(this.stones))}onlineStone(t,n){const a=this.nodes.find(c=>c.stone.stone_id===t);if(!a)return;n&&Object.assign(a.stone,n),a.offline=!1,this._refreshTex(a);const l=a.stone.color||Np;a.bodyMat.color=new _e(l),a.bodyMat.emissive=new _e(yu(a.stone.health)),this._rebuildEdges(),this.onDataChange(this.stones)}setBanks(t){this._clearBanks(),this.banks=[...t];const n=$r(this.banks.length);this.banks.forEach((a,l)=>{const c=new k(...n[l]).multiplyScalar(this.bankRadius);this.bankNodes.push(this._mkBankNode(a,c))}),this.onDataChange(this.stones)}addBank(t){if(this.banks.find(f=>Yi(f)===Yi(t)))return;this.banks.push(t);const n=$r(this.banks.length);this.bankNodes.forEach((f,d)=>{const[m,h,g]=n[d];f.targetPos=new k(m,h,g).multiplyScalar(this.bankRadius)});const a=n[this.banks.length-1],l=new k(...a).multiplyScalar(this.bankRadius),c=this._mkBankNode(t,l);c.enterScale=0,this.bankNodes.push(c),this.layoutProgress=0}removeBank(t){const n=this.bankNodes.findIndex(l=>Yi(l.bank)===t);if(n<0)return;const a=this.bankNodes[n];a.removing=!0,a.removeProgress=0,a.removeCallback=()=>{this.sg.remove(a.group),this.bankHitTargets=this.bankHitTargets.filter(c=>c.userData.bankId!==t),this.bankNodes.splice(this.bankNodes.indexOf(a),1),this.banks=this.banks.filter(c=>Yi(c)!==t);const l=$r(this.banks.length);this.bankNodes.forEach((c,f)=>{c.targetPos=new k(...l[f]).multiplyScalar(this.bankRadius)}),this.layoutProgress=0}}updateBank(t,n){const a=this.bankNodes.find(c=>Yi(c.bank)===t);if(!a)return;Object.assign(a.bank,n);const l=this.banks.findIndex(c=>Yi(c)===t);l>=0&&(this.banks[l]={...this.banks[l],...n}),this._refreshBankTex(a)}setSeedCount(t,n){this.updateBank(t,{_seedCount:n})}resetView(){this.sg.quaternion.identity(),this.camera.position.set(0,2,28),this.camera.lookAt(0,0,0)}destroy(){cancelAnimationFrame(this._animId);const t=this.renderer.domElement;t.removeEventListener("contextmenu",this._bCtx),t.removeEventListener("pointerdown",this._bPD),t.removeEventListener("pointerenter",this._bEnter),t.removeEventListener("pointerleave",this._bLeave),window.removeEventListener("pointermove",this._bPM),window.removeEventListener("pointerup",this._bPU),t.removeEventListener("wheel",this._bWh),window.removeEventListener("resize",this._bRz),this.scene.traverse(n=>{n.geometry&&n.geometry.dispose(),n.material&&(n.material.map&&n.material.map.dispose(),n.material.dispose())}),this.renderer.dispose(),this.container.removeChild(t)}_bindEvents(t){this._bPD=this._onPD.bind(this),this._bPM=this._onPM.bind(this),this._bPU=this._onPU.bind(this),this._bWh=this._onWh.bind(this),this._bCtx=n=>n.preventDefault(),this._bRz=this._resize.bind(this),this._bEnter=()=>{this.mouseInCanvas=!0},this._bLeave=()=>{this.mouseInCanvas=!1},t.addEventListener("contextmenu",this._bCtx),t.addEventListener("pointerdown",this._bPD),t.addEventListener("pointerenter",this._bEnter),t.addEventListener("pointerleave",this._bLeave),window.addEventListener("pointermove",this._bPM),window.addEventListener("pointerup",this._bPU),t.addEventListener("wheel",this._bWh,{passive:!1}),window.addEventListener("resize",this._bRz)}_refreshTex(t){const n=ex(t.stone,t.offline);t.disp.material.map.dispose(),t.disp.material.map=new qs(n),t.disp.material.map.minFilter=Sn,t.disp.material.needsUpdate=!0}_clearAll(){this.nodes.forEach(t=>this.sg.remove(t.group)),this._clearEdges(),this.nodes=[],this.hitTargets=[]}_clearEdges(){this.conns.forEach(t=>{this.sg.remove(t.tube),t.tube.geometry.dispose(),t.tubeMat.dispose(),t.sparkles.forEach(n=>{this.sg.remove(n),n.material.dispose()}),t.label&&(this.sg.remove(t.label),t.labelMat.map.dispose(),t.labelMat.dispose())}),this.conns=[]}_rebuildEdges(){this._clearEdges();const t=this.nodes.filter(a=>!a.removing&&!a.offline),n=t.map(a=>a.stone);LR(n).forEach(a=>{const l=t[a.from],c=t[a.to],f=a.direction===2?c.group.position:l.group.position,d=a.direction===2?l.group.position:c.group.position;this.conns.push(this._mkConn(f,d,a.sets,a.direction))})}_computeRotTarget(t){const n=new k;return t.group.getWorldPosition(n),new wi().setFromUnitVectors(n.clone().normalize(),this.camera.position.clone().normalize()).multiply(this.sg.quaternion.clone())}_toScreen(t){const n=t.clone().project(this.camera),a=this.renderer.domElement.getBoundingClientRect();return{x:(n.x*.5+.5)*a.width,y:(-n.y*.5+.5)*a.height}}_screenOf(t){const n=this.nodes.find(l=>l.stone.stone_id===t);if(n){const l=new k;n.disp.getWorldPosition(l);const c=this._toScreen(l),f=this.camera.position.distanceTo(l),d=this.camera.fov*(Math.PI/180),m=this.renderer.domElement.getBoundingClientRect(),h=n.disp.scale.y/(2*f*Math.tan(d/2))*m.height;return c.y-=h*(61/512),c}const a=this.bankNodes.find(l=>Yi(l.bank)===t);if(a){const l=new k;a.disp.getWorldPosition(l);const c=this._toScreen(l),f=this.camera.position.distanceTo(l),d=this.camera.fov*(Math.PI/180),m=this.renderer.domElement.getBoundingClientRect(),h=a.disp.scale.y/(2*f*Math.tan(d/2))*m.height;return c.y-=h*(32/384),c}return null}_mkNode(t,n){const a=t.color||Np,l=new Qs;l.position.copy(n),this.sg.add(l);const c=new Ev({color:new _e(a),emissive:new _e(yu(t.health)),emissiveIntensity:.4,roughness:.7,metalness:.2,transparent:!0,opacity:1});l.add(new Gn(new eo(.45,20,20),c));const f=new ks({map:ax(a),transparent:!0,opacity:.35,blending:_l,depthWrite:!1}),d=new Zr(f);d.scale.set(3.5,3.5,1),l.add(d);const m=new qs(ex(t));m.minFilter=Sn;const h=new ks({map:m,transparent:!0,depthWrite:!1}),g=new Zr(h);g.position.copy(n.clone().normalize().multiplyScalar(.6)),g.scale.set(4.2,4.2,1),l.add(g);const _=new Gn(new eo(2.2,8,8),new Ws({visible:!1}));return _.userData.stoneId=t.stone_id,l.add(_),this.hitTargets.push(_),{group:l,body:l.children[0],bodyMat:c,glow:d,glowMat:f,disp:g,dispMat:h,pos:n,stone:t,baseScale:4.2,targetPos:null,enterScale:1,offline:!1,removing:!1,removeProgress:0}}_mkBankNode(t,n){const a="#c4b060",l=new Qs;l.position.copy(n),this.sg.add(l);const c=new Ev({color:new _e(a),emissive:new _e(a),emissiveIntensity:.35,roughness:.6,metalness:.3,transparent:!0,opacity:1});l.add(new Gn(new eo(.28,18,18),c));const f=new ks({map:ax(a),transparent:!0,opacity:.3,blending:_l,depthWrite:!1}),d=new Zr(f);d.scale.set(2.6,2.6,1),l.add(d);const m=new qs(nx(t));m.minFilter=Sn;const h=new ks({map:m,transparent:!0,depthWrite:!1}),g=new Zr(h);g.position.copy(n.clone().normalize().multiplyScalar(.5)),g.scale.set(2.8,2.8,1),l.add(g);const _=Yi(t),v=new Gn(new eo(1.4,8,8),new Ws({visible:!1}));return v.userData.bankId=_,l.add(v),this.bankHitTargets.push(v),{kind:"bank",group:l,body:l.children[0],bodyMat:c,glow:d,glowMat:f,disp:g,dispMat:h,pos:n,bank:t,baseScale:2.8,targetPos:null,enterScale:1,removing:!1,removeProgress:0}}_refreshBankTex(t){const n=nx(t.bank);t.disp.material.map.dispose(),t.disp.material.map=new qs(n),t.disp.material.map.minFilter=Sn,t.disp.material.needsUpdate=!0}_clearBanks(){this.bankNodes.forEach(t=>this.sg.remove(t.group)),this.bankNodes=[],this.bankHitTargets=[],this.banks=[]}_mkConn(t,n,a,l=0){const c=l!==0,f=UR(t,n,this.R,48),d=new kx(f),m=c?12890208:8693149,h=c?"#c4b060":"#84a59d",g=new Ws({color:m,transparent:!0,opacity:c?.28:.18,depthWrite:!1}),_=new Gn(new Qp(d,48,.025+a.length*.008,6,!1),g);this.sg.add(_);const v=document.createElement("canvas");v.width=256,v.height=48;const y=v.getContext("2d");y.font='400 16px "IBM Plex Mono",monospace',y.fillStyle=h,y.textAlign="center",y.textBaseline="middle";const E=c?`${a.join(" · ")} ▶`:a.join(" · ");y.fillText(E,128,24);const A=new ks({map:new qs(v),transparent:!0,opacity:.6,depthWrite:!1}),S=new Zr(A);S.position.copy(d.getPoint(.5).normalize().multiplyScalar(this.R*1.06)),S.scale.set(3.5,.7,1),this.sg.add(S);const x=[],w=c?Math.min(a.length+2,4):Math.min(a.length+1,3);for(let D=0;D<w;D++){const U=new ks({map:this.sparkTex,transparent:!0,opacity:c?.85:.7,blending:_l,depthWrite:!1}),G=new Zr(U);G.scale.set(c?.4:.35,c?.4:.35,1),G.userData.t=D/w,G.userData.spd=(c?.11:.08)+Math.random()*.06,G.position.copy(d.getPoint(G.userData.t)),this.sg.add(G),x.push(G)}return{tube:_,tubeMat:g,curve:d,sparkles:x,label:S,labelMat:A,sets:a,directed:c}}_animate(){const t=(performance.now()-this.t0)*.001,n=1/60,a=this.camera.position.z,l=this.mouseInCanvas?0:1;this.autoRotMul+=(l-this.autoRotMul)*.03;const c=this.rotTarget&&this.rotProgress<1;if(c){this.rotProgress=Math.min(this.rotProgress+n/this.rotDuration,1);const d=1-Math.pow(1-this.rotProgress,3);this.sg.quaternion.copy(this.rotFrom.clone().slerp(this.rotTarget,d)),this.rotProgress>=1&&(this.departingId=null)}else if(!this.isDrag&&((Math.abs(this.vel.x)>5e-5||Math.abs(this.vel.y)>5e-5)&&(this.sg.quaternion.premultiply(new wi().setFromAxisAngle(new k(0,1,0),this.vel.x)),this.sg.quaternion.premultiply(new wi().setFromAxisAngle(new k(1,0,0),this.vel.y)),this.vel.x*=.96,this.vel.y*=.96),performance.now()-this.lastInput>3500)){const d=8e-4*this.autoRotMul;d>1e-6&&this.sg.quaternion.premultiply(new wi().setFromAxisAngle(new k(0,1,0),d))}if(this.layoutProgress<1){this.layoutProgress=Math.min(this.layoutProgress+n/.8,1);const d=1-Math.pow(1-this.layoutProgress,3);let m=!1;this.nodes.forEach(h=>{h.targetPos&&!h.removing&&(h.group.position.lerp(h.targetPos,d),h.pos=h.group.position.clone(),h.disp.position.copy(h.pos.clone().normalize().multiplyScalar(.6)),this.layoutProgress>=1&&(h.targetPos=null,m=!0)),h.enterScale<1&&(h.enterScale=Math.min(h.enterScale+n/.6,1),h.group.scale.setScalar(1-Math.pow(1-h.enterScale,2)))}),this.bankNodes.forEach(h=>{h.targetPos&&!h.removing&&(h.group.position.lerp(h.targetPos,d),h.pos=h.group.position.clone(),h.disp.position.copy(h.pos.clone().normalize().multiplyScalar(.5)),this.layoutProgress>=1&&(h.targetPos=null)),h.enterScale<1&&(h.enterScale=Math.min(h.enterScale+n/.6,1),h.group.scale.setScalar(1-Math.pow(1-h.enterScale,2)))}),m&&this._rebuildEdges()}this.nodes.forEach(d=>{if(d.removing){d.removeProgress=Math.min(d.removeProgress+n/.5,1);const m=1-d.removeProgress;d.group.scale.setScalar(m),d.bodyMat.opacity=m,d.dispMat.opacity=m,d.glowMat.opacity=m*.35,d.removeProgress>=1&&d.removeCallback&&(d.removeCallback(),d.removeCallback=null)}}),this.bankNodes.forEach(d=>{if(d.removing){d.removeProgress=Math.min(d.removeProgress+n/.5,1);const m=1-d.removeProgress;d.group.scale.setScalar(m),d.bodyMat.opacity=m,d.dispMat.opacity=m,d.glowMat.opacity=m*.3,d.removeProgress>=1&&d.removeCallback&&(d.removeCallback(),d.removeCallback=null)}}),this.ringMat.opacity=.09+.025*Math.sin(t*.7);const f=new k;this.nodes.forEach(d=>{if(d.removing)return;d.group.getWorldPosition(f);const m=this.camera.position.distanceTo(f),h=a-this.R,g=a+this.R,_=Zi.clamp((m-h)/(g-h),0,1),v=Zi.lerp(1,.08,_),y=Zi.lerp(1,.55,_);d.dispMat.opacity=v,d.disp.scale.setScalar(d.baseScale*y),d.glowMat.opacity=v*.35,d.bodyMat.opacity=v;const E=ay(d.stone.health)&&!d.offline,A=d.stone.health==="thriving"||d.stone.health==="healthy"?.5:d.stone.health==="withering"||d.stone.health==="degraded"?1.3:0,S=E?.25+.25*Math.sin(t*A*Math.PI*2):.08;d.bodyMat.emissiveIntensity=d.offline?.05:S*(1-_*.5),(d.stone.stone_id===this.hoveredId||d.stone.stone_id===this.selectedId)&&(d.glowMat.opacity=Math.min(v*.7,.7),d.disp.scale.setScalar(d.baseScale*y*1.08))}),this.bankNodes.forEach(d=>{if(d.removing)return;d.group.getWorldPosition(f);const m=this.camera.position.distanceTo(f),h=a-this.bankRadius,g=a+this.bankRadius,_=Zi.clamp((m-h)/(g-h),0,1),v=Zi.lerp(1,.18,_),y=Zi.lerp(1,.65,_);d.dispMat.opacity=v,d.disp.scale.setScalar(d.baseScale*y),d.glowMat.opacity=v*.3,d.bodyMat.opacity=v,d.bodyMat.emissiveIntensity=.25+.15*Math.sin(t*.4*Math.PI*2);const E=Yi(d.bank);(E===this.hoveredId||E===this.selectedId)&&(d.glowMat.opacity=Math.min(v*.7,.7),d.disp.scale.setScalar(d.baseScale*y*1.1))}),this.onTrack({selected:this.selectedId?{id:this.selectedId,pos:this._screenOf(this.selectedId)}:null,departing:this.departingId?{id:this.departingId,pos:this._screenOf(this.departingId)}:null,hovered:this.hoveredId?{id:this.hoveredId,pos:this._screenOf(this.hoveredId)}:null,progress:c?this.rotProgress:1}),this.conns.forEach(d=>{if(d.sparkles.forEach(m=>{m.userData.t=(m.userData.t+m.userData.spd*.016)%1,m.position.copy(d.curve.getPoint(m.userData.t)),m.material.opacity=.4+.3*Math.sin(t*2.5+m.userData.t*8)}),d.label){const m=new k;d.label.getWorldPosition(m);const h=this.camera.position.distanceTo(m),g=Zi.clamp((h-(a-this.R))/(a+this.R-(a-this.R)),0,1);d.labelMat.opacity=Zi.lerp(.55,.05,g)}}),this.pLight.position.copy(this.camera.position),this.renderer.render(this.scene,this.camera)}_rayTest(t){const n=this.renderer.domElement.getBoundingClientRect();this.mouse.x=(t.clientX-n.left)/n.width*2-1,this.mouse.y=-((t.clientY-n.top)/n.height)*2+1,this.ray.setFromCamera(this.mouse,this.camera);const a=this.ray.intersectObjects(this.hitTargets);if(a.length>0)return{kind:"stone",id:a[0].object.userData.stoneId};const l=this.ray.intersectObjects(this.bankHitTargets);return l.length>0?{kind:"bank",id:l[0].object.userData.bankId}:null}_onPD(t){if(t.button===2||t.button===1)this.isDrag=!0,this.prevM={x:t.clientX,y:t.clientY},this.vel={x:0,y:0},this.rotProgress=1;else if(t.button===0){const n=this._rayTest(t),a=n?n.id:null,l=n?n.kind:null,c=a===this.selectedId?null:a,f=c?l:null,d=this.selectedId;if(this.selectedId=c,this.selectedKind=f,c){this.departingId=d;const m=f==="bank"?this.bankNodes.find(h=>Yi(h.bank)===c):this.nodes.find(h=>h.stone.stone_id===c);m&&(this.rotFrom=this.sg.quaternion.clone(),this.rotTarget=this._computeRotTarget(m),this.rotProgress=0,this.vel={x:0,y:0}),this.onTransition({selectedId:c,departingId:d,kind:f})}else this.departingId=d,this.onTransition({selectedId:null,departingId:d,kind:null})}this.lastInput=performance.now()}_onPM(t){if(!this.isDrag){const n=this._rayTest(t),a=n?n.id:null,l=n?n.kind:null;a!==this.hoveredId&&(this.hoveredId=a,this.hoveredKind=l,this.onHover(a,l))}if(this.isDrag){const n=t.clientX-this.prevM.x,a=t.clientY-this.prevM.y;this.prevM={x:t.clientX,y:t.clientY};const l=.004;this.sg.quaternion.premultiply(new wi().setFromAxisAngle(new k(0,1,0),n*l)),this.sg.quaternion.premultiply(new wi().setFromAxisAngle(new k(1,0,0),a*l)),this.vel={x:n*l,y:a*l},this.lastInput=performance.now()}}_onPU(t){(t.button===2||t.button===1)&&(this.isDrag=!1)}_onWh(t){t.preventDefault(),this.camera.position.z=Zi.clamp(this.camera.position.z+t.deltaY*.025,16,48),this.camera.lookAt(0,0,0),this.lastInput=performance.now()}_resize(){const t=this.container.clientWidth,n=this.container.clientHeight;this.camera.aspect=t/n,this.camera.updateProjectionMatrix(),this.renderer.setSize(t,n)}_startAnim(){const t=()=>{this._animId=requestAnimationFrame(t),this._animate()};t()}}const sx={percent:null,message:null,status:"unknown",result:null,error:null,operation:null};function zR(r){const[t,n]=ut.useState(sx);return ut.useEffect(()=>{if(!r){n(sx);return}let a=!1;const l=[];return(async()=>(l.push(await $e("job:snapshot",c=>{a||c.payload.id===r&&n(f=>({...f,status:BR(c.payload.status),percent:rx(c.payload.current_step,c.payload.total_steps),message:c.payload.last_message??f.message,result:c.payload.result??f.result,error:c.payload.error??f.error,operation:c.payload.operation??f.operation}))})),l.push(await $e("job:progress",c=>{a||c.payload.job_id===r&&n(f=>({...f,status:f.status==="completed"||f.status==="failed"?f.status:"running",percent:rx(c.payload.step,c.payload.total_steps)??f.percent,message:c.payload.message??f.message}))})),l.push(await $e("job:completed",c=>{a||c.payload.job_id===r&&n(f=>({...f,status:"completed",percent:1,result:c.payload.result}))})),l.push(await $e("job:failed",c=>{a||c.payload.job_id===r&&n(f=>({...f,status:"failed",error:c.payload.error}))}))))(),()=>{a=!0;for(const c of l)c()}},[r]),t}function BR(r){switch(r.toLowerCase()){case"pending":return"pending";case"running":return"running";case"completed":return"completed";case"failed":return"failed";default:return"unknown"}}function rx(r,t){return typeof r!="number"||typeof t!="number"||t<=0?null:Math.max(0,Math.min(1,r/t))}const Du="application/zen-garden+json";function FR({onClose:r}){const t=ut.useRef(null),n=ut.useRef(null),a=ut.useRef(new Set),l=ut.useRef(new Set),[c,f]=ut.useState(null),[d,m]=ut.useState(null),[h,g]=ut.useState(null),[_,v]=ut.useState(null),[y,E]=ut.useState(null),[A,S]=ut.useState([]),[x,w]=ut.useState([]),[D,U]=ut.useState(null),[G,O]=ut.useState([]),[B,R]=ut.useState(null),[z,K]=ut.useState([]),[V,$]=ut.useState([]),[ht,gt]=ut.useState(null),[q,P]=ut.useState({}),[F,ct]=ut.useState(null),J=ut.useCallback(Nt=>({stone_id:Nt.stone_id,stone_name:Nt.stone_name,health:Nt.health}),[]);ut.useEffect(()=>{if(!t.current)return;const Nt=new IR(t.current,{onHover:(Ht,Ut)=>{f(Ht),m(Ut??null)},onTransition:({selectedId:Ht,departingId:Ut,kind:Gt})=>{g(Ht),v(Gt??null)},onTrack:Ht=>E(Ht)});return n.current=Nt,()=>{Nt.destroy(),n.current=null,a.current.clear(),l.current.clear()}},[]),ut.useEffect(()=>{const Nt=n.current;if(!Nt)return;const Ht=new Set(A.map(Gt=>Gt.stone_id)),Ut=a.current;if(Ut.size===0&&A.length>0){Nt.setData(A.map(J)),A.forEach(Gt=>Ut.add(Gt.stone_id));return}A.forEach(Gt=>{Ut.has(Gt.stone_id)?Nt.updateStone(Gt.stone_id,J(Gt)):(Nt.addStone(J(Gt)),Ut.add(Gt.stone_id))}),Array.from(Ut).forEach(Gt=>{Ht.has(Gt)||(Nt.removeStone(Gt),Ut.delete(Gt))})},[A,J]),ut.useEffect(()=>{const Nt=n.current;if(!Nt)return;const Ht=new Set(x.map(Gt=>Gt.name)),Ut=l.current;if(Ut.size===0&&x.length>0){Nt.setBanks(x),x.forEach(Gt=>Ut.add(Gt.name));return}x.forEach(Gt=>{Ut.has(Gt.name)?Nt.updateBank(Gt.name,Gt):(Nt.addBank(Gt),Ut.add(Gt.name))}),Array.from(Ut).forEach(Gt=>{Ht.has(Gt)||(Nt.removeBank(Gt),Ut.delete(Gt))})},[x]);const xt=ut.useCallback(async()=>{try{const[Nt,Ht,Ut,Gt]=await Promise.all([rt("get_topology"),rt("get_tended"),rt("get_storage"),rt("get_offering_sets")]);S(Nt),U(Ht),w(Ut?.banks??[]),O(Gt),R(null)}catch(Nt){R(String(Nt))}},[]);ut.useEffect(()=>{let Nt,Ht,Ut=!1;return(async()=>(await xt(),Nt=await $e("topology-changed",Gt=>{Ut||S(Gt.payload)}),Ht=await $e("tending-changed",Gt=>{Ut||U(Gt.payload)})))(),()=>{Ut=!0,Nt?.(),Ht?.()}},[xt]),ut.useEffect(()=>{let Nt=!1,Ht;return(async()=>Ht=await $e("job:started",Ut=>{if(Nt)return;const{job_id:Gt,fqn:ne}=Ut.payload;P(Me=>{let le=!1;const Ue={};for(const[W,We]of Object.entries(Me))!le&&We.fqn===ne&&We.jobId===null?(Ue[W]={...We,jobId:Gt},le=!0):Ue[W]=We;return Ue})}))(),()=>{Nt=!0,Ht?.()}},[]),ut.useEffect(()=>{if(_!=="stone"){K([]);return}const Nt=A.find(Ut=>Ut.stone_id===h);if(!Nt||Nt.stone_name!==D?.stone_name){K([]);return}let Ht=!1;return(async()=>{try{const Ut=await rt("get_services");Ht||K(Ut?.services??[])}catch(Ut){console.error("get_services failed:",Ut),Ht||K([])}})(),()=>{Ht=!0}},[h,_,D,A]),ut.useEffect(()=>{if(_!=="bank"||!h){$([]);return}let Nt=!1;return(async()=>{try{const Ht=await rt("list_seeds_in_bank",{bankName:h});Nt||($(Ht.seeds),n.current?.setSeedCount(h,Ht.count))}catch(Ht){console.error("list_seeds_in_bank failed:",Ht),Nt||$([])}})(),()=>{Nt=!0}},[h,_]);const I=ut.useCallback(Nt=>!c||!d?null:Nt.kind==="offering"&&d==="bank"?{kind:"bank",id:c}:Nt.kind==="seed"&&d==="stone"?{kind:"stone",id:c}:null,[c,d]),Q=ut.useCallback(Nt=>{Nt.dataTransfer.types.includes(Du)&&(Nt.preventDefault(),Nt.dataTransfer.dropEffect="copy",gt(c))},[c]),Mt=ut.useCallback(()=>{gt(null)},[]),Rt=ut.useCallback(async Nt=>{const Ht=Nt.dataTransfer.getData(Du);if(gt(null),!Ht)return;Nt.preventDefault();let Ut;try{Ut=JSON.parse(Ht)}catch{return}const Gt=I(Ut);if(Gt){if(Ut.kind==="offering"&&Gt.kind==="bank"){const ne=Gt.id,Me=`${Ut.source_stone}::${Ut.fqn}->${ne}`;P(le=>({...le,[Me]:{fqn:Ut.fqn,bankName:ne,jobId:null}}));try{const le=await rt("capture_snapshot",{stone:Ut.source_stone,fqn:Ut.fqn,target:`bank:${ne}`});ct(le);try{const Ue=await rt("list_seeds_in_bank",{bankName:ne});n.current?.setSeedCount(ne,Ue.count),_==="bank"&&h===ne&&$(Ue.seeds)}catch{}}catch(le){R(`Backup failed: ${String(le)}`)}finally{P(le=>{const Ue={...le};return delete Ue[Me],Ue})}return}if(Ut.kind==="seed"&&Gt.kind==="stone"){const ne=A.find(le=>le.stone_id===Gt.id);if(!ne)return;const Me=`${Ut.snapshot_id}->${ne.stone_name}`;P(le=>({...le,[Me]:{fqn:Ut.source_fqn,bankName:Ut.bank_name,jobId:null}}));try{const le=await rt("plant_snapshot",{targetStone:ne.stone_name,targetFqn:Ut.source_fqn,fromSnapshot:Ut.snapshot_id,fromStone:Ut.source_stone,fromFqn:Ut.source_fqn});ct(null),R(null),st(le)}catch(le){R(`Plant failed: ${String(le)}`)}finally{P(le=>{const Ue={...le};return delete Ue[Me],Ue})}return}}},[I,A,_,h]),[wt,st]=ut.useState(null);ut.useEffect(()=>{n.current},[ht]);const bt=ut.useMemo(()=>{const Nt=new Map;for(const Ht of G){const[Ut,Gt]=WR(Ht.name),ne=Ht.members.map(Me=>Me.stone_name);for(const Me of Ht.members){const le={offering:Ut,instance_name:Gt,role:Me.role,is_primary:Ht.primary_stone===Me.stone_name,peer_stones:ne.filter(W=>W!==Me.stone_name)},Ue=Nt.get(Me.stone_name)??[];Ue.push(le),Nt.set(Me.stone_name,Ue)}}return Nt},[G]);ut.useEffect(()=>{const Nt=n.current;if(Nt)for(const Ht of A){const Ut=bt.get(Ht.stone_name)??[];Nt.updateStone(Ht.stone_id,{offerings:Ut.map(Gt=>({offering:Gt.offering,instance_name:Gt.instance_name??null,role:Gt.role??null}))})}},[bt,A]);const Tt=ut.useMemo(()=>_==="stone"?A.find(Nt=>Nt.stone_id===h)??null:null,[A,h,_]),Wt=ut.useMemo(()=>_==="bank"?x.find(Nt=>Nt.name===h)??null:null,[x,h,_]),re=ut.useMemo(()=>d==="stone"?A.find(Nt=>Nt.stone_id===c)??null:null,[A,c,d]),ie=ut.useMemo(()=>d==="bank"?x.find(Nt=>Nt.name===c)??null:null,[x,c,d]);return b.jsxs("main",{className:"content canvas-content",children:[b.jsxs("header",{className:"topbar canvas-topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"}),b.jsx("span",{className:"canvas-stone-count",children:A.length===0?"no stones in earshot":`${A.length} stone${A.length===1?"":"s"}`})]}),B&&b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"Error"}),b.jsx("div",{className:"placeholder-body",children:B})]}),b.jsxs("div",{className:"canvas-stage",children:[b.jsx("div",{ref:t,className:`canvas-mount${ht?" canvas-mount-drop-active":""}`,onDragOver:Q,onDragLeave:Mt,onDrop:Rt}),Tt&&y?.selected&&b.jsx(GR,{stone:Tt,tendedName:D?.stone_name,position:y.selected.pos,services:z,setMemberships:bt.get(Tt.stone_name)??[],onDismiss:()=>{n.current?.resetView(),g(null),v(null)}}),Wt&&y?.selected&&b.jsx(kR,{bank:Wt,position:y.selected.pos,seeds:V,onDismiss:()=>{n.current?.resetView(),g(null),v(null)}}),c&&c!==h&&y?.hovered&&b.jsx(XR,{label:d==="bank"?ie?.name:re?Dp(re.stone_name):void 0,position:y.hovered.pos,kind:d??"stone"})]}),F&&b.jsx(ox,{message:`Snapshot captured: ${F.source_fqn} (${sy(F.size_total_bytes)})`,onDismiss:()=>ct(null)}),wt&&b.jsx(ox,{message:`Planted ${wt.target_fqn}${wt.digest_drift==="drift"?" (manifest drift)":""}`,onDismiss:()=>st(null)}),Object.keys(q).length>0&&b.jsx("div",{className:"canvas-forming-rail",children:Object.entries(q).map(([Nt,Ht])=>b.jsx(HR,{info:Ht},Nt))}),b.jsx("footer",{className:"canvas-hint",children:"Right-drag to rotate · scroll to zoom · drag an offering to a bank to back it up"})]})}function HR({info:r}){const t=zR(r.jobId),n=t.message??`${r.fqn} → ${r.bankName}`,a=t.percent??0,l=t.percent!==null;return b.jsxs("div",{className:`seed seed-forming${l?" seed-forming-with-progress":""}`,title:`${r.fqn} → ${r.bankName} (${l?`${Math.round(a*100)}%`:"starting"}): ${n}`,style:{"--seed-progress":String(a)},children:[b.jsx("span",{className:"seed-glyph","aria-hidden":!0,children:"◆"}),b.jsxs("span",{className:"seed-label",children:[r.fqn," → ",r.bankName,l&&b.jsxs("span",{className:"seed-progress-pct",children:[" · ",Math.round(a*100),"%"]})]})]})}function ox({message:r,onDismiss:t}){return ut.useEffect(()=>{const n=setTimeout(t,5e3);return()=>clearTimeout(n)},[t]),b.jsx("div",{className:"canvas-toast",role:"status",onClick:t,children:r})}function GR({stone:r,tendedName:t,position:n,services:a,setMemberships:l,onDismiss:c}){const f=t===r.stone_name;return b.jsxs("div",{className:"canvas-card",style:{left:`${n.x}px`,top:`${n.y}px`},children:[b.jsxs("header",{className:"canvas-card-header",children:[b.jsx("span",{className:`dot ${qR(r.health)}`}),b.jsx("span",{className:"canvas-card-title",children:Dp(r.stone_name)}),b.jsx("button",{type:"button",className:"canvas-card-close",onClick:c,"aria-label":"Close",children:"×"})]}),b.jsxs("dl",{className:"canvas-card-body",children:[b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Endpoint"}),b.jsx("dd",{className:"kv-value-mono",children:r.endpoint})]}),b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Health"}),b.jsx("dd",{children:r.health})]}),b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Services"}),b.jsx("dd",{children:r.services_count})]}),b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Last seen"}),b.jsx("dd",{children:ry(r.age_secs)})]})]}),f&&b.jsx("div",{className:"canvas-card-tended-pill",children:"tended"}),l.length>0&&b.jsxs("section",{className:"canvas-card-sets",children:[b.jsx("div",{className:"canvas-card-section-title",children:"Sets"}),b.jsx("ul",{className:"canvas-card-set-list",children:l.map(d=>b.jsxs("li",{className:"canvas-card-set-row",children:[b.jsxs("span",{className:"canvas-card-set-fqn",children:[d.offering,d.instance_name&&b.jsxs("span",{className:"canvas-card-set-instance",children:["::",d.instance_name]})]}),b.jsx("span",{className:`canvas-card-set-role canvas-card-set-role-${d.role??"unknown"}`,title:`Role on this set: ${d.role??"unknown"}`,children:d.role??"—"}),d.peer_stones.length>0&&b.jsxs("span",{className:"canvas-card-set-peers",title:`Peers: ${d.peer_stones.map(Dp).join(", ")}`,children:["+",d.peer_stones.length]})]},`${d.offering}${d.instance_name??""}`))})]}),a.length>0&&b.jsxs("section",{className:"canvas-card-offerings",children:[b.jsx("div",{className:"canvas-card-section-title",children:"Drag to a bank to back up"}),b.jsx("div",{className:"canvas-card-offering-chips",children:a.map(d=>b.jsx(VR,{stoneName:r.stone_name,service:d},d.name))})]})]})}function VR({stoneName:r,service:t}){const n=a=>{const l={kind:"offering",source_stone:r,fqn:t.name,display_name:t.offering};a.dataTransfer.setData(Du,JSON.stringify(l)),a.dataTransfer.effectAllowed="copy"};return b.jsxs("div",{className:`canvas-offering-chip status-${t.status}`,draggable:!0,onDragStart:n,title:`Drag to bank to snapshot ${t.name}`,children:[b.jsx("span",{className:`canvas-offering-chip-dot status-${t.status}`,"aria-hidden":!0}),b.jsx("span",{className:"canvas-offering-chip-label",children:t.offering})]})}function kR({bank:r,position:t,seeds:n,onDismiss:a}){return b.jsxs("div",{className:"canvas-card canvas-card-bank",style:{left:`${t.x}px`,top:`${t.y}px`},children:[b.jsxs("header",{className:"canvas-card-header",children:[b.jsx("span",{className:"canvas-card-bank-glyph","aria-hidden":!0,children:"◆"}),b.jsx("span",{className:"canvas-card-title",children:r.name}),b.jsx("button",{type:"button",className:"canvas-card-close",onClick:a,"aria-label":"Close",children:"×"})]}),b.jsxs("dl",{className:"canvas-card-body",children:[b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Replicas"}),b.jsx("dd",{children:r.replica_count})]}),b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Primary"}),b.jsx("dd",{className:r.primary_stone?"kv-value-mono":"",children:r.primary_stone??"—"})]}),b.jsxs("div",{className:"canvas-card-row",children:[b.jsx("dt",{children:"Roles"}),b.jsx("dd",{children:r.roles.length===0?"—":r.roles.join(" · ")})]})]}),n.length>0&&b.jsxs("section",{className:"canvas-card-seeds",children:[b.jsx("div",{className:"canvas-card-section-title",children:"Drag a seed to a stone to plant"}),b.jsx("div",{className:"canvas-card-seed-list",children:n.map(l=>b.jsx(jR,{seed:l,bankName:r.name},l.snapshot_id))})]})]})}function jR({seed:r,bankName:t}){const n=a=>{const l={kind:"seed",snapshot_id:r.snapshot_id,source_fqn:r.source_fqn,source_stone:r.source_stone,bank_name:t};a.dataTransfer.setData(Du,JSON.stringify(l)),a.dataTransfer.effectAllowed="copy"};return b.jsxs("div",{className:"seed seed-draggable",draggable:!0,onDragStart:n,title:`Drag to a stone to plant ${r.source_fqn} (${sy(r.size_total_bytes)})`,children:[b.jsx("span",{className:"seed-glyph","aria-hidden":!0,children:"◆"}),b.jsxs("span",{className:"seed-label",children:[r.source_fqn,b.jsxs("span",{className:"seed-meta",children:[" · ",YR(r.created_at)]})]})]})}function XR({label:r,position:t,kind:n}){return r?b.jsx("div",{className:`canvas-hover-chip canvas-hover-chip-${n}`,style:{left:`${t.x}px`,top:`${t.y}px`},children:r}):null}function Dp(r){return r.startsWith("stone-")?r.slice(6):r}function WR(r){const t=r.indexOf("::");return t<0?[r,void 0]:[r.slice(0,t),r.slice(t+2)]}function qR(r){const t=r.toLowerCase();return t==="healthy"||t==="thriving"?"dot-ok":t==="degraded"||t==="withering"?"dot-amber":t==="unhealthy"||t==="down"||t==="offline"?"dot-down":""}function sy(r){if(!r)return"0B";const t=["B","K","M","G","T","P"];let n=0,a=r;for(;a>=1024&&n<t.length-1;)a/=1024,n+=1;return`${a.toFixed(a<10&&n>0?1:0)}${t[n]}`}function YR(r){const t=new Date(r).getTime(),n=Date.now(),a=Math.max(0,Math.floor((n-t)/1e3));return ry(a)}function ry(r){return r<60?`${r}s ago`:r<3600?`${Math.floor(r/60)}m ago`:r<86400?`${Math.floor(r/3600)}h ago`:`${Math.floor(r/86400)}d ago`}function ZR(r,t){if(r.length===0)return 0;const n=r.toLowerCase(),a=t.toLowerCase();let l=0,c=-1,f=0;for(let d=0;d<a.length&&l<n.length;d++)a[d]===n[l]&&(c>=0&&(f+=d-c),c=d,l++);return l<n.length?null:f}function KR({onClose:r,onNavigate:t}){const[n,a]=ut.useState(""),[l,c]=ut.useState([]),[f,d]=ut.useState(null),[m,h]=ut.useState(null),[g,_]=ut.useState(0),v=ut.useRef(null),y=ut.useRef(null);ut.useEffect(()=>{v.current?.focus(),(async()=>{try{const[w,D]=await Promise.all([rt("get_topology"),rt("get_tended")]);c(w),h(D)}catch{}try{const w=await rt("get_services");d(w)}catch{}})()},[]);const E=ut.useMemo(()=>{const w=[];w.push({id:"nav:home",label:"Open Home",hint:"destination",kind:"navigate",view:"home"},{id:"nav:services",label:"Open Services",hint:"destination",kind:"navigate",view:"services"},{id:"nav:pond",label:"Open Pond",hint:"destination",kind:"navigate",view:"pond"},{id:"nav:activity",label:"Open Activity",hint:"destination",kind:"navigate",view:"activity"},{id:"nav:settings",label:"Open Settings",hint:"destination",kind:"navigate",view:"settings"});for(const D of l)m?.stone_name!==D.stone_name&&w.push({id:`tend:${D.stone_id}`,label:`Tend ${D.stone_name}`,hint:D.endpoint,kind:"tend",stone_id:D.stone_id});if(f&&m)for(const D of f.services){const U=D.status.toLowerCase()==="running";U||w.push({id:`wake:${D.name}`,label:`Wake ${D.name}`,hint:`${D.offering} on ${m.stone_name}`,kind:"service",op:"wake",name:D.name}),U&&w.push({id:`rest:${D.name}`,label:`Rest ${D.name}`,hint:`${D.offering} on ${m.stone_name}`,kind:"service",op:"rest",name:D.name}),w.push({id:`restart:${D.name}`,label:`Restart ${D.name}`,hint:`${D.offering} on ${m.stone_name}`,kind:"service",op:"restart",name:D.name})}return w},[l,f,m]),A=ut.useMemo(()=>{if(n.trim().length===0)return E;const w=n.trim(),D=[];for(const U of E){const G=`${U.label} ${U.hint}`,O=ZR(w,G);O!==null&&D.push({action:U,score:O})}return D.sort((U,G)=>U.score-G.score),D.map(U=>U.action)},[E,n]);ut.useEffect(()=>{_(0)},[A.length]),ut.useEffect(()=>{const w=y.current;if(!w)return;w.querySelector(`[data-palette-idx="${g}"]`)?.scrollIntoView({block:"nearest"})},[g]);const S=ut.useCallback(async w=>{try{switch(w.kind){case"navigate":t(w.view),r();return;case"tend":await rt("set_tended",{stoneId:w.stone_id}),r();return;case"service":{const D=w.op==="wake"?"wake_service":w.op==="rest"?"rest_service":"restart_service";await rt(D,{name:w.name}),r();return}}}catch(D){console.error("palette action failed:",D)}},[r,t]),x=ut.useCallback(w=>{if(w.key==="Escape"){w.preventDefault(),r();return}if(w.key==="ArrowDown"){w.preventDefault(),_(D=>Math.min(D+1,Math.max(A.length-1,0)));return}if(w.key==="ArrowUp"){w.preventDefault(),_(D=>Math.max(D-1,0));return}if(w.key==="Enter"){w.preventDefault();const D=A[g];D&&S(D)}},[S,r,A,g]);return b.jsx("div",{className:"palette-backdrop",onClick:r,role:"dialog","aria-modal":"true","aria-label":"Command palette",children:b.jsxs("div",{className:"palette",onClick:w=>w.stopPropagation(),onKeyDown:x,children:[b.jsx("input",{ref:v,className:"palette-input",type:"text",value:n,onChange:w=>a(w.target.value),placeholder:"Type a destination, stone, or service…","aria-label":"Command palette input",spellCheck:!1,autoComplete:"off"}),b.jsx("div",{className:"palette-results",ref:y,children:A.length===0?b.jsx("div",{className:"palette-empty",children:"No matches"}):A.map((w,D)=>b.jsxs("button",{type:"button","data-palette-idx":D,className:`palette-row ${D===g?"palette-row-selected":""}`,onMouseEnter:()=>_(D),onClick:()=>void S(w),children:[b.jsx("span",{className:"palette-row-label",children:w.label}),b.jsx("span",{className:"palette-row-hint",children:w.hint})]},w.id))}),b.jsxs("div",{className:"palette-footer",children:[b.jsx("span",{children:"↑↓ navigate"}),b.jsx("span",{className:"sep",children:"·"}),b.jsx("span",{children:"↵ run"}),b.jsx("span",{className:"sep",children:"·"}),b.jsx("span",{children:"esc close"})]})]})})}var lx;(function(r){r.Nsis="nsis",r.Msi="msi",r.Deb="deb",r.Rpm="rpm",r.AppImage="appimage",r.App="app"})(lx||(lx={}));async function QR(){return rt("plugin:app|version")}function JR({onNavigate:r}){const[t,n]=ut.useState(null),[a,l]=ut.useState(!1),c=ut.useCallback(async()=>{try{const h=await rt("get_suggestion");n(h)}catch(h){console.error("get_suggestion failed:",h)}},[]);ut.useEffect(()=>{let h,g=!1;return(async()=>(await c(),h=await $e("suggestion-changed",_=>{g||n(_.payload)})))(),()=>{g=!0,h?.()}},[c]);const f=ut.useCallback(async()=>{if(t){l(!0);try{switch(t.action.kind){case"tend":await rt("set_tended",{stoneId:t.action.stone_id});break;case"open_view":r(t.action.view);break}}catch(h){console.error("facilitator action failed:",h)}finally{l(!1)}}},[r,t]),d=ut.useCallback(async()=>{if(t){l(!0);try{await rt("dismiss_suggestion",{id:t.id})}catch(h){console.error("dismiss_suggestion failed:",h)}finally{l(!1)}}},[t]),m=ut.useCallback(async()=>{if(t){l(!0);try{await rt("hide_suggestion_kind",{kind:t.kind})}catch(h){console.error("hide_suggestion_kind failed:",h)}finally{l(!1)}}},[t]);return t?b.jsxs("section",{className:"facilitator",children:[b.jsx("div",{className:"facilitator-glyph",children:"💡"}),b.jsxs("div",{className:"facilitator-body",children:[b.jsx("div",{className:"facilitator-title",children:t.title}),b.jsx("div",{className:"facilitator-text",children:t.body})]}),b.jsxs("div",{className:"facilitator-actions",children:[b.jsx("button",{type:"button",className:"facilitator-primary",onClick:f,disabled:a,children:t.action_label}),b.jsx("button",{type:"button",className:"facilitator-secondary",onClick:d,disabled:a,children:"Not now"}),b.jsx("button",{type:"button",className:"facilitator-tertiary",onClick:m,disabled:a,title:"Don't suggest this kind again",children:"Hide this kind"})]})]}):null}function $R({onNavigate:r}){const[t,n]=ut.useState("…"),[a,l]=ut.useState(new Date().toLocaleTimeString()),[c,f]=ut.useState([]),[d,m]=ut.useState(null),[h,g]=ut.useState(null),[_,v]=ut.useState(null),[y,E]=ut.useState(null),[A,S]=ut.useState(null),[x,w]=ut.useState(null),[D,U]=ut.useState(null),[G,O]=ut.useState([]),B=ut.useCallback(async()=>{try{const J=await rt("get_services");g(J),v(null)}catch(J){v(String(J)),g(null)}try{const J=await rt("get_pond_status");E(J),S(null)}catch(J){S(String(J)),E(null)}try{const J=await rt("get_storage");w(J),U(null)}catch(J){U(String(J)),w(null)}},[]),R=ut.useCallback(async()=>{try{const J=await rt("get_activity");O(J)}catch(J){console.error("get_activity failed:",J)}},[]);ut.useEffect(()=>{let J,xt,I,Q=!1;(async()=>{try{const[wt,st,bt]=await Promise.all([rt("get_topology"),rt("get_tended"),rt("get_activity")]);if(Q)return;f(wt),m(st),O(bt),st&&B()}catch(wt){console.error("initial load failed:",wt)}J=await $e("topology-changed",wt=>{f(wt.payload)}),xt=await $e("tending-changed",wt=>{m(wt.payload),B()}),I=await $e("activity-changed",()=>{R()})})(),QR().then(n).catch(()=>n("?"));const Rt=setInterval(()=>l(new Date().toLocaleTimeString()),1e3);return()=>{Q=!0,J?.(),xt?.(),I?.(),clearInterval(Rt)}},[B,R]);const z=ut.useCallback(async J=>{try{await rt("set_tended",{stoneId:J.stone_id})}catch(xt){console.error("set_tended failed:",xt)}},[]),K=d?`tending ${d.stone_name}`:c.length>0?`${c.length} stone${c.length===1?"":"s"} aware · auto-tending…`:"no garden yet",V=String(c.length),$=c.length===0?"listening for chirps…":"chirping in earshot · TTL 90s",ht=d?_?"!":h===null?"…":String(h.count):"—",gt=d?_?"fetch failed":h===null?"fetching…":h.count===0?"no offerings on this stone":`running on ${d.stone_name}`:"no stone tended",q=d?A?"!":y===null?"…":y.initialised?y.member_count!==null?String(y.member_count):"•":"—":"—",P=d?A?"fetch failed":y===null?"fetching…":y.initialised?y.name?y.name:y.status:"no pond on this stone":"no stone tended",F=d?D?"!":x===null?"…":String(x.count):"—",ct=d?D?"fetch failed":x===null?"fetching…":x.count===0?"no banks reachable":x.count===1?"1 bank in this garden":`${x.count} banks in this garden`:"no stone tended";return b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("div",{className:"garden-pill",children:K}),b.jsx("div",{className:"topbar-spacer"}),b.jsx("div",{className:"topbar-clock",children:a})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Pavilion is running."}),b.jsxs("p",{className:"subtle",children:["v",t," · awareness via UDP chirps · tending via ~/.zen-garden/.tending"]})]}),b.jsx(JR,{onNavigate:r}),b.jsxs("section",{className:"tiles",children:[b.jsxs("article",{className:"tile",children:[b.jsx("div",{className:"tile-label",children:"Stones"}),b.jsx("div",{className:"tile-value",children:V}),b.jsx("div",{className:"tile-foot",children:$})]}),b.jsxs("article",{className:"tile",children:[b.jsx("div",{className:"tile-label",children:"Storage"}),b.jsx("div",{className:"tile-value",children:F}),b.jsx("div",{className:"tile-foot",children:ct})]}),b.jsxs("article",{className:"tile",children:[b.jsx("div",{className:"tile-label",children:"Services"}),b.jsx("div",{className:"tile-value",children:ht}),b.jsx("div",{className:"tile-foot",children:gt})]}),b.jsxs("article",{className:"tile",children:[b.jsx("div",{className:"tile-label",children:"Pond"}),b.jsx("div",{className:"tile-value",children:q}),b.jsx("div",{className:"tile-foot",children:P})]})]}),c.length>0&&b.jsxs("section",{className:"stones-list",children:[b.jsx("div",{className:"stones-list-title",children:"Aware stones · click to tend"}),c.map(J=>{const xt=d?.stone_name===J.stone_name;return b.jsxs("button",{className:`stone-row ${xt?"stone-row-tended":""}`,onClick:()=>z(J),disabled:xt,title:xt?"Currently tended":`Tend ${J.stone_name}`,children:[b.jsx("span",{className:"stone-name",children:J.stone_name}),b.jsx("span",{className:"stone-endpoint",children:J.endpoint}),b.jsxs("span",{className:"stone-age",children:[J.age_secs,"s"]})]},J.stone_id)})]}),h&&h.services.length>0&&b.jsxs("section",{className:"stones-list",children:[b.jsxs("div",{className:"stones-list-title",children:["Services on ",d.stone_name]}),h.services.map(J=>b.jsxs("div",{className:"stone-row",style:{cursor:"default"},children:[b.jsx("span",{className:"stone-name",children:J.name}),b.jsx("span",{className:"stone-endpoint",children:J.offering}),b.jsx("span",{className:"stone-age",children:J.status})]},J.name))]}),x&&x.banks.length>0&&b.jsxs("section",{className:"stones-list",children:[b.jsx("div",{className:"stones-list-title",children:"Banks across the garden"}),x.banks.map(J=>{const xt=J.replica_count===1?"1 replica":`${J.replica_count} replicas`,I=J.primary_stone?`primary · ${J.primary_stone}`:J.roles&&J.roles.length>0?J.roles.join(", "):"—";return b.jsxs("div",{className:"stone-row",style:{cursor:"default"},children:[b.jsx("span",{className:"stone-name",children:J.name}),b.jsx("span",{className:"stone-endpoint",children:xt}),b.jsx("span",{className:"stone-age",children:I})]},J.name)})]}),G.length>0&&b.jsxs("section",{className:"stones-list",children:[b.jsx("div",{className:"stones-list-title",children:"Recent activity"}),G.slice(0,12).map(J=>{const{primary:xt,secondary:I}=tC(J.event);return b.jsxs("div",{className:"stone-row",style:{cursor:"default"},children:[b.jsxs("span",{className:"stone-name",children:[b.jsx("span",{className:`severity-pip severity-${J.severity}`}),xt]}),b.jsx("span",{className:"stone-endpoint",children:I}),b.jsx("span",{className:"stone-age",children:eC(J.at)})]},J.id)})]}),b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"Awareness · API integration"}),b.jsxs("div",{className:"placeholder-body",children:["Topology is push-driven from ",b.jsx("code",{children:"STONE_CHIRP"})," + provoked",b.jsx("code",{children:" DISCOVERY_RESPONSE"}),". Services, pond, and storage are pull-on-demand against the tended stone (refresh on every",b.jsx("code",{children:" tending-changed"}),"). Tending file shared with Rake at",b.jsx("code",{children:" ~/.zen-garden/.tending"}),". Toasts respect Settings → quiet hours and Hide-this-kind. Cloud Filter and companions arrive in the next milestone."]})]})]})}function tC(r){switch(r.kind){case"stone_joined":return{primary:`${r.stone_name} joined`,secondary:r.endpoint};case"stone_left":return{primary:`${r.stone_name} offline`,secondary:"lost contact"};case"storage_activity":{const t=r.creates+r.modifies+r.deletes;return{primary:`${r.bank_name} synced ${t} files`,secondary:`${r.creates} new · ${r.modifies} changed · ${r.deletes} removed`}}}}function eC(r){const t=Date.now()-new Date(r).getTime();if(t<0)return"just now";const n=Math.floor(t/1e3);if(n<60)return`${n}s`;const a=Math.floor(n/60);if(a<60)return`${a}m`;const l=Math.floor(a/60);return l<24?`${l}h`:`${Math.floor(l/24)}d`}function nC({onComplete:r}){const[t,n]=ut.useState([]),[a,l]=ut.useState(!1),[c,f]=ut.useState(null);ut.useEffect(()=>{let h,g=!1;return(async()=>{try{const _=await rt("get_topology");g||n(_)}catch(_){g||f(String(_))}h=await $e("topology-changed",_=>{g||n(_.payload)})})(),()=>{g=!0,h?.()}},[]);const d=ut.useCallback(async h=>{l(!0),f(null);try{await rt("set_tended",{stoneId:h.stone_id}),await rt("set_settings",{patch:{onboarded:!0}}),r()}catch(g){f(String(g)),l(!1)}},[r]),m=ut.useCallback(async()=>{l(!0),f(null);try{await rt("set_settings",{patch:{onboarded:!0}}),r()}catch(h){f(String(h)),l(!1)}},[r]);return b.jsx("div",{className:"onboarding",children:b.jsxs("div",{className:"onboarding-frame",children:[b.jsxs("header",{className:"onboarding-head",children:[b.jsx("div",{className:"onboarding-mark",children:"P"}),b.jsxs("div",{children:[b.jsx("h1",{className:"onboarding-title",children:"Welcome to Pavilion"}),b.jsx("p",{className:"onboarding-sub",children:"Pavilion sits in your tray and watches the garden you have on the network. Pick a stone to anchor to — you can always switch later."})]})]}),c&&b.jsx("div",{className:"onboarding-error",children:c}),b.jsx("section",{className:"onboarding-list",children:t.length===0?b.jsxs("div",{className:"onboarding-empty",children:[b.jsx("div",{className:"onboarding-empty-spinner","aria-hidden":"true"}),b.jsxs("div",{children:[b.jsx("strong",{children:"Listening for stones…"}),b.jsx("p",{className:"subtle",children:"Pavilion is already broadcasting a discovery probe. Stones on this LAN should show up within a few seconds."})]})]}):t.map(h=>b.jsxs("button",{type:"button",className:"onboarding-stone",onClick:()=>d(h),disabled:a,children:[b.jsx("span",{className:"onboarding-stone-dot dot dot-ok"}),b.jsx("span",{className:"onboarding-stone-name",children:h.stone_name}),b.jsx("span",{className:"onboarding-stone-endpoint",children:h.endpoint}),b.jsx("span",{className:"onboarding-stone-cta",children:a?"…":"Tend"})]},h.stone_id))}),b.jsx("footer",{className:"onboarding-foot",children:b.jsx("button",{type:"button",className:"onboarding-skip",onClick:m,disabled:a,children:"Skip — let Pavilion auto-tend"})})]})})}const iC={init:"Place keystone — initialise pond",join:"Join pond",invite:"Open enrollment",unlock:"Unlock pond"};function aC({kind:r,onClose:t}){const[n,a]=ut.useState(null),[l,c]=ut.useState(!1),[f,d]=ut.useState(null),[m,h]=ut.useState({}),[g,_]=ut.useState({}),v=ut.useRef(null);ut.useEffect(()=>{let S=!1;return(async()=>{c(!0);try{const x=await rt("ceremony_step",{request:{session_id:null,ceremony:r,data:{}}});if(S)return;a(x),d(null)}catch(x){S||d(String(x))}finally{S||c(!1)}})(),()=>{S=!0}},[r]),ut.useEffect(()=>{h({}),_({}),setTimeout(()=>v.current?.focus(),0)},[n?.session_id,n?.prompts.length]);const y=ut.useCallback(async()=>{if(n){for(const S of n.prompts)if(S.input_type==="secret_confirm"){const x=m[S.key]??"",w=g[S.key]??"";if(x!==w){d(`'${S.label}' values do not match`);return}}c(!0),d(null);try{const S=await rt("ceremony_step",{request:{session_id:n.session_id,ceremony:null,data:m}});a(S)}catch(S){d(String(S))}finally{c(!1)}}},[g,m,n]),E=ut.useCallback(S=>{S.key==="Escape"&&n?.complete&&(S.preventDefault(),t()),S.key==="Enter"&&!S.shiftKey&&n&&n.prompts.length>0&&!l&&(S.preventDefault(),y())},[l,t,n,y]),A=iC[r]??`Ceremony: ${r}`;return b.jsx("div",{className:"ceremony-backdrop",role:"dialog","aria-modal":"true","aria-label":A,onClick:S=>{S.target===S.currentTarget&&(!n||n.complete)&&t()},children:b.jsxs("div",{className:"ceremony",onKeyDown:E,children:[b.jsxs("header",{className:"ceremony-head",children:[b.jsx("span",{className:"ceremony-mark",children:"P"}),b.jsx("span",{className:"ceremony-title",children:A}),n?.complete?b.jsx("button",{type:"button",className:"ceremony-close",onClick:t,"aria-label":"Close",children:"✕"}):b.jsx("span",{className:"ceremony-step-indicator",children:l?"working…":"in progress"})]}),f&&b.jsx("div",{className:"ceremony-error",role:"alert",children:f}),n?b.jsxs(b.Fragment,{children:[n.messages.map((S,x)=>b.jsx(sC,{message:S},`${n.session_id}-msg-${x}`)),n.error&&!n.complete&&b.jsx("div",{className:"ceremony-error",role:"alert",children:n.error}),!n.complete&&n.prompts.length>0&&b.jsx(oC,{prompts:n.prompts,inputs:m,onInput:(S,x)=>h(w=>({...w,[S]:x})),confirmInputs:g,onConfirmInput:(S,x)=>_(w=>({...w,[S]:x})),firstInputRef:v,disabled:l})]}):b.jsx("div",{className:"ceremony-loading",children:"Starting ceremony…"}),b.jsx("footer",{className:"ceremony-foot",children:n?.complete?b.jsxs(b.Fragment,{children:[b.jsx("span",{className:"ceremony-foot-status",children:n.error?"Failed":"Complete"}),b.jsx("button",{type:"button",className:"ceremony-primary",onClick:t,children:"Done"})]}):b.jsxs(b.Fragment,{children:[b.jsx("button",{type:"button",className:"ceremony-secondary",onClick:t,disabled:l,children:"Cancel"}),n&&n.prompts.length>0&&b.jsx("button",{type:"button",className:"ceremony-primary",onClick:y,disabled:l,children:l?"Submitting…":"Continue"})]})})]})})}function sC({message:r}){switch(r.kind){case"qr_code":return b.jsx(rC,{message:r});case"summary":return b.jsxs("section",{className:"ceremony-msg ceremony-msg-summary",children:[b.jsx("div",{className:"ceremony-msg-title",children:r.title}),b.jsx("pre",{className:"ceremony-msg-content",children:r.content})]});case"error":return b.jsxs("section",{className:"ceremony-msg ceremony-msg-error",children:[b.jsx("div",{className:"ceremony-msg-title",children:r.title}),b.jsx("div",{className:"ceremony-msg-content",children:r.content})]});case"info":default:return b.jsxs("section",{className:"ceremony-msg ceremony-msg-info",children:[r.title&&b.jsx("div",{className:"ceremony-msg-title",children:r.title}),b.jsx("div",{className:"ceremony-msg-content",children:r.content})]})}}function rC({message:r}){const t=r.content.trim(),n=/^[A-Za-z0-9+/=\s]+$/.test(t)&&t.length>100,a=t.startsWith("otpauth://")||t.startsWith("http");return b.jsxs("section",{className:"ceremony-msg ceremony-msg-qr",children:[r.title&&b.jsx("div",{className:"ceremony-msg-title",children:r.title}),n?b.jsx("img",{className:"ceremony-qr-img",src:`data:image/png;base64,${t}`,alt:"QR code"}):a?b.jsx("code",{className:"ceremony-qr-uri",children:t}):b.jsx("pre",{className:"ceremony-qr-pre",children:r.content})]})}function oC({prompts:r,inputs:t,onInput:n,confirmInputs:a,onConfirmInput:l,firstInputRef:c,disabled:f}){return b.jsx("section",{className:"ceremony-prompts",children:r.map((d,m)=>b.jsx(lC,{prompt:d,value:t[d.key]??"",onChange:h=>n(d.key,h),confirmValue:a[d.key]??"",onConfirmChange:h=>l(d.key,h),inputRef:m===0?c:null,disabled:f},d.key))})}function lC({prompt:r,value:t,onChange:n,confirmValue:a,onConfirmChange:l,inputRef:c,disabled:f}){const d=ut.useMemo(()=>cC(r.input_type),[r.input_type]);return r.input_type==="select_one"?b.jsxs("div",{className:"ceremony-prompt",children:[b.jsx("label",{className:"ceremony-prompt-label",children:r.label}),r.help&&b.jsx("div",{className:"ceremony-prompt-help",children:r.help}),b.jsx("div",{className:"ceremony-prompt-options",children:(r.options??[]).map(m=>b.jsxs("label",{className:"ceremony-option",children:[b.jsx("input",{type:"radio",name:r.key,value:m.value,checked:t===m.value,onChange:h=>n(h.target.value),disabled:f}),b.jsxs("span",{className:"ceremony-option-label",children:[b.jsx("span",{children:m.label}),m.description&&b.jsx("span",{className:"ceremony-option-desc",children:m.description})]})]},m.value))})]}):b.jsxs("div",{className:"ceremony-prompt",children:[b.jsx("label",{className:"ceremony-prompt-label",htmlFor:`prompt-${r.key}`,children:r.label}),r.help&&b.jsx("div",{className:"ceremony-prompt-help",children:r.help}),b.jsx("input",{id:`prompt-${r.key}`,ref:c,type:d,className:"ceremony-prompt-input",value:t,onChange:m=>n(m.target.value),autoComplete:d==="password"?"new-password":"off",spellCheck:!1,disabled:f}),r.input_type==="secret_confirm"&&b.jsxs(b.Fragment,{children:[b.jsx("label",{className:"ceremony-prompt-label",htmlFor:`prompt-${r.key}-confirm`,children:r.confirm_label??"Confirm"}),b.jsx("input",{id:`prompt-${r.key}-confirm`,type:"password",className:"ceremony-prompt-input",value:a,onChange:m=>l(m.target.value),autoComplete:"new-password",spellCheck:!1,disabled:f})]})]})}function cC(r){switch(r){case"secret":case"secret_confirm":return"password";case"code":case"entropy":return"text";case"text":default:return"text"}}const uC={active:"dot dot-ok",healthy:"dot dot-ok",locked:"dot dot-amber",inactive:"dot dot-amber",uninitialised:"dot",unknown:"dot"};function fC(r){return uC[r.toLowerCase()]??"dot"}function dC({onClose:r}){const[t,n]=ut.useState(null),[a,l]=ut.useState(null),[c,f]=ut.useState(null),[d,m]=ut.useState(null),h=ut.useCallback(async()=>{try{const[g,_]=await Promise.all([rt("get_pond_status"),rt("get_tended")]);n(g),l(_),f(null)}catch(g){f(String(g))}},[]);return ut.useEffect(()=>{let g,_=!1;return(async()=>(await h(),g=await $e("tending-changed",()=>{_||h()})))(),()=>{_=!0,g?.()}},[h]),b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Pond"}),b.jsx("p",{className:"subtle",children:a?`security ceremonies bound to ${a.stone_name}`:"no stone tended"})]}),c&&b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"Error"}),b.jsx("div",{className:"placeholder-body",children:c})]}),a?t?t.initialised?b.jsxs(b.Fragment,{children:[b.jsxs("section",{className:"settings-group",children:[b.jsx("div",{className:"settings-group-title",children:"Status"}),b.jsxs("div",{className:"pond-status-row",children:[b.jsx("span",{className:fC(t.status)}),b.jsx("span",{className:"pond-status-label",children:t.status})]})]}),b.jsxs("section",{className:"settings-group",children:[b.jsx("div",{className:"settings-group-title",children:"Identity"}),b.jsx(Oh,{k:"Pond name",v:t.name??"—",mono:t.name!==null}),b.jsx(Oh,{k:"Cornerstone",v:t.cornerstone??"—",mono:t.cornerstone!==null}),b.jsx(Oh,{k:"Members",v:t.member_count!==null?`${t.member_count}`:"unknown"})]}),b.jsxs("section",{className:"pond-actions",children:[b.jsx("button",{type:"button",className:"pond-action-primary",onClick:()=>m("invite"),children:"Open enrollment"}),b.jsx("button",{type:"button",className:"pond-action-secondary",onClick:()=>m("unlock"),disabled:t.status.toLowerCase()==="active",title:t.status.toLowerCase()==="active"?"Already unlocked":"Unlock the pond after a stone restart",children:"Unlock pond"})]})]}):b.jsxs(b.Fragment,{children:[b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"No pond on this stone"}),b.jsx("div",{className:"placeholder-body",children:"Place a keystone to initialise the pond — Pavilion will walk you through the ceremony."})]}),b.jsxs("section",{className:"pond-actions",children:[b.jsx("button",{type:"button",className:"pond-action-primary",onClick:()=>m("init"),children:"Place keystone"}),b.jsx("button",{type:"button",className:"pond-action-secondary",onClick:()=>m("join"),children:"Join an existing pond"})]})]}):b.jsx("section",{className:"settings-empty",children:"Loading…"}):b.jsx("section",{className:"settings-empty",children:"Tend a stone from the Home view to see its pond."}),d&&b.jsx(aC,{kind:d,onClose:()=>{m(null),h()}})]})}function Oh({k:r,v:t,mono:n=!1}){return b.jsxs("div",{className:"settings-row settings-row-pill",children:[b.jsx("span",{className:"settings-row-label",children:r}),b.jsx("span",{className:n?"kv-value-mono":"kv-value",children:t})]})}const hC={running:"dot dot-ok",stopped:"dot dot-amber",degraded:"dot dot-down",failed:"dot dot-down"};function pC(r){return hC[r.toLowerCase()]??"dot"}function mC({onClose:r}){const[t,n]=ut.useState(null),[a,l]=ut.useState(null),[c,f]=ut.useState([]),[d,m]=ut.useState(null),[h,g]=ut.useState(null),[_,v]=ut.useState({}),y=ut.useCallback(async()=>{try{const[A,S,x]=await Promise.all([rt("get_services"),rt("get_tended"),rt("get_storage")]);n(A),l(S),f(x?.banks??[]),m(null)}catch(A){m(String(A))}},[]);ut.useEffect(()=>{let A,S=!1;return(async()=>(await y(),A=await $e("tending-changed",()=>{S||y()})))(),()=>{S=!0,A?.()}},[y]);const E=ut.useCallback(async(A,S)=>{v(x=>({...x,[A]:S}));try{await rt(S==="wake"?"wake_service":S==="rest"?"rest_service":"restart_service",{name:A}),setTimeout(()=>{y()},600)}catch(x){m(`${S} ${A}: ${String(x)}`)}finally{v(x=>{const w={...x};return delete w[A],w})}},[y]);return b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Services"}),b.jsx("p",{className:"subtle",children:a?`running on ${a.stone_name}`:"no stone tended"})]}),d&&b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"Error"}),b.jsx("div",{className:"placeholder-body",children:d})]}),h&&a&&b.jsx(gC,{service:h,stoneName:a.stone_name,banks:c,onCancel:()=>g(null),onCapture:async A=>{const S=h;g(null),v(x=>({...x,[S.name]:"backup"}));try{const x=await rt("capture_snapshot",{stone:a.stone_name,fqn:S.name,target:A});m(`Snapshot ${x.snapshot_id.slice(0,8)}… captured (${x.source_fqn})`)}catch(x){m(`Backup failed: ${String(x)}`)}finally{v(x=>{const w={...x};return delete w[S.name],w})}}}),a?t?t.count===0?b.jsxs("section",{className:"settings-empty",children:["No offerings on ",a.stone_name,". Plant one with"," ",b.jsxs("code",{children:["garden-rake plant ","<offering>"]}),"."]}):b.jsx("section",{className:"services-grid",children:t.services.map(A=>{const S=pC(A.status),x=A.status.toLowerCase()==="running",w=_[A.name];return b.jsxs("article",{className:"service-card",children:[b.jsxs("header",{className:"service-card-head",children:[b.jsx("span",{className:S}),b.jsx("span",{className:"service-card-name",children:A.name}),b.jsx("span",{className:"service-card-status",children:A.status})]}),b.jsx("div",{className:"service-card-meta",children:b.jsx("span",{className:"service-card-offering",children:A.offering||"—"})}),b.jsxs("footer",{className:"service-card-actions",children:[b.jsx("button",{type:"button",disabled:w!==void 0||x,onClick:()=>E(A.name,"wake"),children:w==="wake"?"starting…":"Wake"}),b.jsx("button",{type:"button",disabled:w!==void 0||!x,onClick:()=>E(A.name,"rest"),children:w==="rest"?"stopping…":"Rest"}),b.jsx("button",{type:"button",disabled:w!==void 0,onClick:()=>E(A.name,"restart"),children:w==="restart"?"restarting…":"Restart"}),b.jsx("button",{type:"button",disabled:w!==void 0,onClick:()=>g(A),title:"Capture a snapshot of this offering",children:w==="backup"?"backing up…":"Backup…"})]})]},A.name)})}):b.jsx("section",{className:"settings-empty",children:"Loading…"}):b.jsx("section",{className:"settings-empty",children:"Tend a stone from the Home view to see its services."})]})}function gC({service:r,stoneName:t,banks:n,onCancel:a,onCapture:l}){const c=[{value:"local",label:"Local disk",note:"<data_dir>/snapshots/"},...n.map(m=>({value:`bank:${m.name}`,label:m.name,note:`${m.replica_count} replica${m.replica_count===1?"":"s"}`}))],[f,d]=ut.useState(0);return ut.useEffect(()=>{function m(h){if(h.key==="Escape"){h.preventDefault(),a();return}if(h.key==="ArrowDown"){h.preventDefault(),d(g=>Math.min(g+1,c.length-1));return}if(h.key==="ArrowUp"){h.preventDefault(),d(g=>Math.max(g-1,0));return}if(h.key==="Enter"){h.preventDefault(),l(c[f].value);return}}return window.addEventListener("keydown",m),()=>window.removeEventListener("keydown",m)},[f,c,a,l]),b.jsx("div",{className:"modal-scrim",role:"dialog","aria-modal":"true",children:b.jsxs("div",{className:"modal-card backup-picker",children:[b.jsxs("header",{className:"modal-header",children:[b.jsxs("h2",{children:["Back up ",r.offering||r.name]}),b.jsx("button",{type:"button",className:"modal-close",onClick:a,"aria-label":"Close",children:"×"})]}),b.jsxs("p",{className:"modal-sub",children:["Capture a snapshot from ",b.jsx("code",{children:t})," and place it…"]}),b.jsx("ul",{className:"backup-target-list",children:c.map((m,h)=>b.jsxs("li",{className:`backup-target${h===f?" focused":""}`,onMouseEnter:()=>d(h),onClick:()=>l(m.value),children:[b.jsx("span",{className:"backup-target-label",children:m.label}),b.jsx("span",{className:"backup-target-note",children:m.note})]},m.value))}),b.jsx("footer",{className:"modal-footer",children:b.jsx("span",{className:"modal-hint",children:"↑↓ navigate · Enter pick · Esc cancel"})})]})})}const _C={stone_joined:"Stone joined the garden",stone_left:"Stone offline",storage_activity:"Storage sync activity"};function vC(r){return _C[r]??r}function cx(r){return/^[0-2]\d:[0-5]\d$/.test(r)}function xC({onClose:r}){const[t,n]=ut.useState(null),[a,l]=ut.useState(null),[c,f]=ut.useState(!1),d=ut.useCallback(async()=>{try{const _=await rt("get_settings");n(_),l(null)}catch(_){l(String(_))}},[]);ut.useEffect(()=>{let _,v=!1;return(async()=>(await d(),_=await $e("settings-changed",y=>{v||n(y.payload)})))(),()=>{v=!0,_?.()}},[d]);const m=ut.useCallback(async _=>{f(!0);try{const v=await rt("set_settings",{patch:_});n(v),l(null)}catch(v){l(String(v))}finally{f(!1)}},[]),h=ut.useMemo(()=>t?cx(t.quiet_hours.start):!0,[t]),g=ut.useMemo(()=>t?cx(t.quiet_hours.end):!0,[t]);return t?b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"}),b.jsx("div",{className:"topbar-clock",children:c?"saving…":"saved"})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Settings"}),b.jsx("p",{className:"subtle",children:"calm by default · quiet hours and per-source suppression"})]}),a&&b.jsxs("section",{className:"placeholder-note",children:[b.jsx("div",{className:"placeholder-title",children:"Error"}),b.jsx("div",{className:"placeholder-body",children:a})]}),b.jsxs("section",{className:"settings-group",children:[b.jsx("div",{className:"settings-group-title",children:"Notifications"}),b.jsxs("label",{className:"settings-row",children:[b.jsx("input",{type:"checkbox",checked:t.quiet_hours.enabled,onChange:_=>m({quiet_hours:{enabled:_.target.checked}}),disabled:c}),b.jsx("span",{className:"settings-row-label",children:"Quiet hours"}),b.jsx("span",{className:"settings-row-help",children:"Suppress toasts during the configured window. Activity is still logged."})]}),b.jsxs("div",{className:"settings-row settings-row-time",children:[b.jsx("span",{className:"settings-row-label",children:"Window"}),b.jsx("input",{type:"time",value:t.quiet_hours.start,onChange:_=>m({quiet_hours:{start:_.target.value}}),disabled:c||!t.quiet_hours.enabled,className:h?"":"settings-input-invalid","aria-invalid":!h}),b.jsx("span",{className:"settings-row-sep",children:"to"}),b.jsx("input",{type:"time",value:t.quiet_hours.end,onChange:_=>m({quiet_hours:{end:_.target.value}}),disabled:c||!t.quiet_hours.enabled,className:g?"":"settings-input-invalid","aria-invalid":!g}),b.jsx("span",{className:"settings-row-help",children:"Wraps over midnight when end is earlier than start."})]})]}),b.jsxs("section",{className:"settings-group",children:[b.jsx("div",{className:"settings-group-title",children:"Hidden notification kinds"}),t.suppressed_kinds.length===0?b.jsx("div",{className:"settings-empty",children:'None hidden. Future "Hide this kind" actions will surface here so you can re-enable them.'}):t.suppressed_kinds.map(_=>b.jsxs("div",{className:"settings-row settings-row-pill",children:[b.jsx("span",{className:"settings-row-label",children:vC(_)}),b.jsx("button",{type:"button",className:"settings-row-action",disabled:c,onClick:()=>m({suppressed_kinds:t.suppressed_kinds.filter(v=>v!==_)}),children:"Show again"})]},_))]}),b.jsxs("section",{className:"settings-group",children:[b.jsx("div",{className:"settings-group-title",children:"Startup"}),b.jsxs("label",{className:"settings-row",children:[b.jsx("input",{type:"checkbox",checked:t.autostart_enabled,onChange:_=>m({autostart_enabled:_.target.checked}),disabled:c}),b.jsx("span",{className:"settings-row-label",children:"Start Pavilion when I sign in"}),b.jsx("span",{className:"settings-row-help",children:"Adds Pavilion to your user-level autostart entries. The OS state is reconciled on every change, including this one."})]})]})]}):b.jsxs("main",{className:"content",children:[b.jsxs("header",{className:"topbar",children:[b.jsx("button",{className:"garden-pill",onClick:r,type:"button",children:"← Home"}),b.jsx("div",{className:"topbar-spacer"})]}),b.jsxs("section",{className:"hero",children:[b.jsx("h1",{children:"Settings"}),b.jsx("p",{className:"subtle",children:a??"Loading…"})]})]})}function yC(){const[r,t]=ut.useState("home"),[n,a]=ut.useState([]),[l,c]=ut.useState(null),[f,d]=ut.useState(null),[m,h]=ut.useState(!1);ut.useEffect(()=>{let A,S,x,w=!1;return(async()=>{try{const[U,G,O]=await Promise.all([rt("get_topology"),rt("get_tended"),rt("get_settings")]);if(w)return;a(U),c(G),d(O)}catch(U){console.error("shell initial load failed:",U)}A=await $e("topology-changed",U=>{a(U.payload)}),S=await $e("tending-changed",U=>{c(U.payload)}),x=await $e("settings-changed",U=>{d(U.payload)})})(),()=>{w=!0,A?.(),S?.(),x?.()}},[]);const g=ut.useMemo(()=>l?n.some(A=>A.stone_name===l.stone_name||A.endpoint===l.endpoint):!1,[n,l]),_=g?"dot dot-ok":l?"dot dot-down":n.length>0?"dot dot-amber":"dot",v=g?`connected to ${l.stone_name}`:l?`${l.stone_name} silent`:n.length>0?"selecting tended stone…":"no garden in earshot",y=f?.quiet_hours.enabled?`quiet hours ${f.quiet_hours.start}–${f.quiet_hours.end}`:"quiet hours off",E=ut.useCallback(()=>t("home"),[]);return ut.useEffect(()=>{function A(S){(S.ctrlKey||S.metaKey)&&(S.key==="k"||S.key==="K")&&(S.preventDefault(),h(x=>!x))}return window.addEventListener("keydown",A),()=>{window.removeEventListener("keydown",A)}},[]),f&&!f.onboarded?b.jsx(nC,{onComplete:()=>{d(A=>A&&{...A,onboarded:!0})}}):b.jsxs("div",{className:"pavilion-shell",children:[b.jsxs("aside",{className:"sidebar",children:[b.jsxs("div",{className:"brand",children:[b.jsx("div",{className:"brand-mark",children:"P"}),b.jsx("div",{className:"brand-name",children:"Pavilion"})]}),b.jsxs("nav",{className:"nav",children:[b.jsx("a",{className:`nav-item ${r==="home"?"active":""}`,onClick:()=>t("home"),children:"Home"}),b.jsx("a",{className:`nav-item ${r==="garden"?"active":""}`,onClick:()=>t("garden"),children:"Garden"}),b.jsx("a",{className:"nav-item disabled",children:"Storage"}),b.jsx("a",{className:`nav-item ${r==="services"?"active":""}`,onClick:()=>t("services"),children:"Services"}),b.jsx("a",{className:"nav-item disabled",children:"Companions"}),b.jsx("a",{className:`nav-item ${r==="pond"?"active":""}`,onClick:()=>t("pond"),children:"Pond"}),b.jsx("a",{className:`nav-item ${r==="activity"?"active":""}`,onClick:()=>t("activity"),children:"Activity"}),b.jsx("div",{className:"nav-spacer"}),b.jsx("a",{className:`nav-item ${r==="settings"?"active":""}`,onClick:()=>t("settings"),children:"Settings"})]})]}),r==="home"&&b.jsx($R,{onNavigate:A=>{(A==="home"||A==="garden"||A==="services"||A==="pond"||A==="settings")&&t(A)}}),r==="garden"&&b.jsx(FR,{onClose:E}),r==="services"&&b.jsx(mC,{onClose:E}),r==="pond"&&b.jsx(dC,{onClose:E}),r==="activity"&&b.jsx(wM,{onClose:E}),r==="settings"&&b.jsx(xC,{onClose:E}),b.jsxs("footer",{className:"statusbar",children:[b.jsx("span",{className:_}),v,b.jsx("span",{className:"sep",children:"·"}),b.jsx("span",{children:y}),b.jsx("span",{className:"sep",children:"·"}),b.jsx("span",{className:"statusbar-shortcut",children:"⌃K palette"})]}),m&&b.jsx(KR,{onClose:()=>h(!1),onNavigate:A=>t(A)})]})}const SC=4;function MC(){const[r,t]=ut.useState([]),[n,a]=ut.useState(null),[l,c]=ut.useState([]),[f,d]=ut.useState(null),m=ut.useCallback(async()=>{try{const[y,E,A,S]=await Promise.all([rt("get_topology"),rt("get_tended"),rt("get_activity"),rt("get_suggestion")]);t(y),a(E),c(A),d(S)}catch(y){console.error("popover refresh failed:",y)}},[]);ut.useEffect(()=>{let y,E,A,S,x=!1;return(async()=>(await m(),y=await $e("topology-changed",()=>{x||m()}),E=await $e("tending-changed",()=>{x||m()}),A=await $e("activity-changed",()=>{x||m()}),S=await $e("suggestion-changed",()=>{x||m()})))(),()=>{x=!0,y?.(),E?.(),A?.(),S?.()}},[m]);const h=ut.useMemo(()=>n?r.some(y=>y.stone_name===n.stone_name||y.endpoint===n.endpoint):!1,[r,n]),g=ut.useMemo(()=>l.slice(0,SC),[l]),_=ut.useCallback(async()=>{try{await rt("show_main_window")}catch(y){console.error("show_main_window failed:",y)}try{await Pp().hide()}catch{}},[]),v=ut.useCallback(async y=>{try{await rt("dismiss_suggestion",{id:y})}catch(E){console.error("dismiss_suggestion failed:",E)}},[]);return b.jsxs("div",{className:"popover-shell",children:[b.jsxs("header",{className:"popover-header",children:[b.jsx("div",{className:"brand-mark popover-brand-mark",children:"P"}),b.jsx("div",{className:"popover-title",children:"Pavilion"})]}),b.jsxs("section",{className:"popover-status",children:[b.jsx("span",{className:bC(h,n,r)}),b.jsx("span",{className:"popover-status-text",children:EC(h,n,r)})]}),f&&b.jsxs("section",{className:"popover-suggestion",children:[b.jsx("div",{className:"popover-suggestion-title",children:f.title}),b.jsx("div",{className:"popover-suggestion-body",children:f.body}),b.jsxs("div",{className:"popover-suggestion-actions",children:[b.jsx("button",{type:"button",className:"popover-cta-primary",onClick:()=>void _(),children:f.cta_label}),b.jsx("button",{type:"button",className:"popover-cta-ghost",onClick:()=>void v(f.id),children:"Dismiss"})]})]}),b.jsxs("section",{className:"popover-recent",children:[b.jsx("div",{className:"popover-section-title",children:"Recent"}),g.length===0?b.jsx("div",{className:"popover-empty",children:"No activity yet."}):b.jsx("ul",{className:"popover-recent-list",children:g.map(y=>b.jsxs("li",{className:`popover-recent-row sev-${y.severity}`,children:[b.jsx("span",{className:"popover-recent-time",children:TC(y.at)}),b.jsx("span",{className:"popover-recent-text",children:AC(y.event)})]},y.id))})]}),b.jsx("footer",{className:"popover-footer",children:b.jsx("button",{type:"button",className:"popover-cta-primary popover-cta-block",onClick:()=>void _(),children:"Open Pavilion"})})]})}function bC(r,t,n){return r?"dot dot-ok":t?"dot dot-down":n.length>0?"dot dot-amber":"dot"}function EC(r,t,n){return r?`connected to ${t.stone_name}`:t?`${t.stone_name} silent`:n.length>0?`${n.length} stone${n.length===1?"":"s"} in earshot`:"no garden in earshot"}function TC(r){return new Date(r).toLocaleTimeString(void 0,{hour:"2-digit",minute:"2-digit"})}function AC(r){switch(r.kind){case"stone_joined":return`${r.stone_name} joined`;case"stone_left":return`${r.stone_name} offline`;case"storage_activity":{const t=r.creates+r.modifies+r.deletes;return`${r.bank_name} synced ${t} file${t===1?"":"s"}`}}}const oy=Pp().label==="popover"?"popover":"main";oy==="popover"&&document.body.classList.add("popover-body");xM.createRoot(document.getElementById("root")).render(b.jsx(ut.StrictMode,{children:oy==="popover"?b.jsx(MC,{}):b.jsx(yC,{})}));
//# sourceMappingURL=index-3fVU2siN.js.map

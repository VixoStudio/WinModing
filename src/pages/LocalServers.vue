<script setup>
import { onMounted, onUnmounted, ref } from 'vue'
import { Button } from '@modrinth/ui'
import { useBreadcrumbs } from '@/store/breadcrumbs'
import { useI18n } from '@/helpers/i18n.js'
import { handleError } from '@/store/notifications.js'
import { listLocalServers, deleteLocalServer, installLocalServer, localServerRunning, onLocalServerLog, saveLocalServer, startLocalServer, stopLocalServer } from '@/helpers/localServers.js'

const { t: i18n } = useI18n()
useBreadcrumbs().setRootContext({ name: i18n('localServers.title'), link: '/local-servers' })
const servers = ref([]), logs = ref([]), running = ref({}), busy = ref(false)
let unlisten
const draft = ref({ name: '', kind: 'paper', gameVersion: '1.21.4', build: '', port: 25565, eulaAccepted: false })
async function refresh() {
  try {
    servers.value = await listLocalServers()
    const states = await Promise.all(servers.value.map(async server => [server.id, await localServerRunning(server.id)]))
    running.value = Object.fromEntries(states)
  } catch (error) { handleError(error) }
}
async function create() {
  try {
    busy.value = true
    const record = await saveLocalServer({ ...draft.value, id: '', jarPath: null, createdUnix: 0 })
    servers.value.unshift(record)
    draft.value = { ...draft.value, name: '', build: '' }
  } catch (error) { handleError(error) } finally { busy.value = false }
}
async function install(server) { try { busy.value = true; const path = await installLocalServer(server); await saveLocalServer({ ...server, jarPath: path }); await refresh() } catch (error) { handleError(error) } finally { busy.value = false } }
async function toggle(server) {
  try {
    if (running.value[server.id]) await stopLocalServer(server.id)
    else { if (!server.eulaAccepted) { if (!window.confirm(i18n('localServers.eulaConfirm'))) return; server = await saveLocalServer({ ...server, eulaAccepted: true }) }; await startLocalServer(server) }
    await refresh()
  } catch (error) { handleError(error) }
}
async function remove(server) { try { await deleteLocalServer(server.id); await refresh() } catch (error) { handleError(error) } }
onMounted(async () => {
  await refresh()
  try { unlisten = await onLocalServerLog((entry) => { logs.value.push(entry); if (logs.value.length > 400) logs.value.splice(0, logs.value.length - 400) }) }
  catch (error) { handleError(error) }
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <main class="local-servers">
    <header class="heading"><div><h1>{{ i18n('localServers.title') }}</h1><p>{{ i18n('localServers.subtitle') }}</p></div><Button @click="refresh">{{ i18n('localServers.refresh') }}</Button></header>
    <form class="create" @submit.prevent="create">
      <label>{{ i18n('localServers.name') }}<input v-model="draft.name" required maxlength="64" :placeholder="i18n('localServers.namePlaceholder')" /></label>
      <label>{{ i18n('localServers.type') }}<select v-model="draft.kind"><option value="vanilla">Vanilla</option><option value="paper">Paper</option><option value="fabric">Fabric</option></select></label>
      <label>{{ i18n('localServers.version') }}<input v-model="draft.gameVersion" required placeholder="1.21.4" /></label>
      <label v-if="draft.kind === 'paper'">{{ i18n('localServers.paperProject') }}<select v-model="draft.build"><option value="">{{ i18n('localServers.paperLatest') }}</option><option value="folia">{{ i18n('localServers.foliaLatest') }}</option></select></label>
      <label v-else-if="draft.kind === 'fabric'">{{ i18n('localServers.fabricLoader') }}<input v-model="draft.build" :placeholder="i18n('localServers.fabricLatest')" /></label>
      <label v-else>{{ i18n('localServers.type') }}<input disabled :value="i18n('localServers.vanillaMetadata')" /></label>
      <label>{{ i18n('localServers.port') }}<input v-model.number="draft.port" type="number" min="1" max="65535" /></label>
      <Button type="submit" :disabled="busy">{{ i18n('localServers.add') }}</Button>
    </form>
    <section class="server-list">
      <article v-for="server in servers" :key="server.id" class="server-card">
        <div class="server-copy"><h2>{{ server.name }}</h2><p>{{ server.kind }} · Minecraft {{ server.gameVersion }} · {{ i18n('localServers.port') }} {{ server.port }}</p></div>
        <div class="actions"><Button @click="install(server)" :disabled="busy || running[server.id]">{{ i18n('localServers.download') }}</Button><Button @click="toggle(server)">{{ running[server.id] ? i18n('localServers.stop') : i18n('localServers.start') }}</Button><Button color="danger" @click="remove(server)">{{ i18n('localServers.delete') }}</Button></div>
      </article>
      <p v-if="!servers.length" class="empty">{{ i18n('localServers.empty') }}</p>
    </section>
    <section class="console"><h2>{{ i18n('localServers.console') }}</h2><pre><template v-for="(entry, index) in logs" :key="index">[{{ entry.serverId }}] {{ entry.line }}
</template></pre></section>
  </main>
</template>

<style scoped lang="scss">
.local-servers{display:flex;flex-direction:column;gap:1rem;padding:1.25rem;max-width:1120px;margin:0 auto}.heading,.server-card{display:flex;align-items:center;justify-content:space-between;gap:1rem}.heading h1,.heading p,.server-card h2,.server-card p{margin:0}.heading p,.server-card p{color:var(--color-secondary);margin-top:.35rem}.create{display:grid;grid-template-columns:repeat(auto-fit,minmax(175px,1fr));align-items:end;gap:.85rem;padding:1rem;border:1px solid var(--color-button-bg);border-radius:var(--radius-md);background:var(--color-raised-bg)}.create label{display:flex;flex-direction:column;gap:.4rem;font-size:.9rem}.create input,.create select{min-width:0;padding:.6rem .7rem;border:1px solid var(--color-button-bg);border-radius:var(--radius-md);background:var(--color-bg);color:var(--color-contrast)}.server-list{display:grid;gap:.65rem}.server-card{padding:.9rem 1rem;border:1px solid var(--color-button-bg);border-radius:var(--radius-md);background:var(--color-raised-bg)}.server-copy{min-width:0}.actions{display:flex;gap:.45rem;flex-wrap:wrap;justify-content:flex-end}.empty{padding:2rem;text-align:center;color:var(--color-secondary)}.console{margin-top:.5rem}.console pre{max-height:260px;overflow:auto;white-space:pre-wrap;background:var(--color-bg);padding:.85rem;border-radius:var(--radius-md);font-size:.8rem;color:var(--color-secondary)}@media(max-width:720px){.heading,.server-card{align-items:flex-start;flex-direction:column}.actions{justify-content:flex-start}}
</style>

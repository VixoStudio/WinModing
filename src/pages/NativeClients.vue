<script setup>
import { onMounted, ref } from 'vue'
import { Button } from '@modrinth/ui'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@/helpers/i18n.js'
import { handleError } from '@/store/notifications.js'
import { getNativeClientRelease, installNativeClient } from '@/helpers/nativeClients.js'

const { t: i18n } = useI18n()
const client = ref(null)
const busy = ref(false)
const progress = ref('')

async function refresh() {
  try {
    client.value = await getNativeClientRelease('win')
  } catch (error) {
    handleError(error)
  }
}

async function install() {
  if (busy.value || !client.value) return
  busy.value = true
  progress.value = i18n('nativeClients.downloading')
  try {
    const path = await installNativeClient('win')
    await invoke('save_native_client_profile', {
      name: i18n('nativeClients.profileName'),
      client: 'win',
      server: null,
    })
    client.value.installedPath = path
    progress.value = i18n('nativeClients.profileReady')
  } catch (error) {
    progress.value = ''
    handleError(error)
  } finally {
    busy.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <main class="native-clients">
    <header>
      <h1>{{ i18n('nativeClients.title') }}</h1>
      <p>{{ i18n('nativeClients.description') }}</p>
    </header>
    <p v-if="progress" class="progress">{{ progress }}</p>
    <article v-if="client" class="client-card">
      <div>
        <h2>{{ i18n('nativeClients.winName') }}</h2>
        <p>{{ i18n('nativeClients.release', { tag: client.tag }) }}</p>
        <p v-if="client.installedPath" class="installed">
          {{ i18n('nativeClients.installed', { path: client.installedPath }) }}
        </p>
      </div>
      <Button :disabled="busy || !!client.installedPath" @click="install">
        {{ busy ? i18n('nativeClients.downloading') : client.installedPath ? i18n('nativeClients.installedButton') : i18n('nativeClients.install') }}
      </Button>
    </article>
  </main>
</template>

<style scoped lang="scss">
.native-clients{display:flex;flex-direction:column;gap:1rem;padding:1.25rem;max-width:1000px;margin:0 auto}.native-clients header{max-width:760px;margin-bottom:.5rem}.native-clients header p,.client-card p{color:var(--color-secondary)}.client-card{padding:1.1rem 1.25rem;border:1px solid var(--color-button-bg);border-radius:var(--radius-md);background:var(--color-raised-bg);display:flex;justify-content:space-between;align-items:center;gap:1rem}.client-card h2,.client-card p{margin:0 0 .5rem}.installed{overflow-wrap:anywhere}.progress{color:var(--color-secondary)}@media(max-width:680px){.client-card{align-items:flex-start;flex-direction:column}}
</style>

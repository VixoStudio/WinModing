<template>
  <div class="instance-container">
    <div class="side-cards pb-4">
      <Card class="instance-card" @contextmenu.prevent.stop="handleRightClick">
        <Avatar
          :key="instance.iconDataUrl || instance.icon_path || instance.path"
          size="md"
          :src="instanceIconSrc(instance)"
          :tint-by="instance.path"
        />
        <div class="instance-info">
          <h2 class="name">{{ instance.name }}</h2>
          <span class="metadata">{{ instance.native_client ? i18n(`create.client.${instance.native_client}`) : `${instance.loader} ${instance.game_version}` }}</span>
          <span v-if="playtimeLabel" class="metadata playtime">{{ playtimeLabel }}</span>
        </div>
        <span class="button-group">
          <Button v-if="instance.install_stage !== 'installed' && !instance.native_client" disabled class="instance-button">
            {{ i18n('instance.installing') }}
          </Button>
          <Button
            v-else-if="playing === true || nativeRunning"
            color="danger"
            class="instance-button"
            @click="stopInstance('InstancePage')"
          >
            <StopCircleIcon />
            {{ i18n('instance.stop') }}
          </Button>
          <Button
            v-else-if="(playing === false && loading === false) || instance.native_client"
            color="primary"
            class="instance-button"
            @click="startInstance('InstancePage')"
          >
            <PlayIcon />
            {{ i18n('instance.play') }}
          </Button>
          <Button
            v-else-if="loading === true && playing === false"
            disabled
            class="instance-button"
          >
            {{ i18n('instance.loading') }}
          </Button>
          <Button
            v-tooltip="'Open instance folder'"
            class="instance-button"
            @click="showProfileInFolder(instance.path)"
          >
            <FolderOpenIcon />
            {{ i18n('instance.folder') }}
          </Button>
        </span>
        <hr class="card-divider" />
        <SlideRail class="pages-list">
          <RouterLink :to="`/instance/${encodeURIComponent($route.params.id)}/`" class="btn">
            <BoxIcon />
            {{ i18n('instance.content') }}
          </RouterLink>
          <RouterLink :to="`/instance/${encodeURIComponent($route.params.id)}/logs`" class="btn">
            <FileIcon />
            {{ i18n('instance.logs') }}
          </RouterLink>
          <RouterLink :to="`/instance/${encodeURIComponent($route.params.id)}/servers`" class="btn">
            <ServerIcon />
            {{ i18n('instance.servers') }}
          </RouterLink>
          <RouterLink :to="`/instance/${encodeURIComponent($route.params.id)}/options`" class="btn">
            <SettingsIcon />
            {{ i18n('instance.options') }}
          </RouterLink>
        </SlideRail>
      </Card>
    </div>
    <div class="content">
      <Card v-if="childError" class="child-error">{{ childError }}</Card>
      <RouterView v-slot="{ Component }">
        <template v-if="Component">
          <Suspense @pending="loadingBar.startLoading()" @resolve="loadingBar.stopLoading()">
            <component
              :is="Component"
              :instance="instance"
              :options="options"
              :offline="offline"
              :playing="playing"
              :versions="modrinthVersions"
              :installed="instance.install_stage !== 'installed'"
            ></component>
            <template #fallback>
              <Card>{{ i18n('instance.loading') }}</Card>
            </template>
          </Suspense>
        </template>
      </RouterView>
    </div>
  </div>
  <ContextMenu ref="options" @option-clicked="handleOptionsClick">
    <template #play> <PlayIcon /> {{ i18n('instance.contextPlay') }} </template>
    <template #stop> <StopCircleIcon /> {{ i18n('instance.contextStop') }} </template>
    <template #add_content> <PlusIcon /> {{ i18n('instance.contextAddContent') }} </template>
    <template #edit> <EditIcon /> {{ i18n('instance.contextEdit') }} </template>
    <template #copy_path> <ClipboardCopyIcon /> {{ i18n('instance.contextCopyPath') }} </template>
    <template #open_folder> <ClipboardCopyIcon /> {{ i18n('instance.contextOpenFolder') }} </template>
    <template #copy_link> <ClipboardCopyIcon /> {{ i18n('instance.contextCopyLink') }} </template>
    <template #open_link> <ClipboardCopyIcon /> {{ i18n('instance.contextOpenProject') }} <ExternalIcon /> </template>
    <template #copy_names><EditIcon />{{ i18n('instance.contextCopyNames') }}</template>
    <template #copy_slugs><HashIcon />{{ i18n('instance.contextCopySlugs') }}</template>
    <template #copy_links><GlobeIcon />{{ i18n('instance.contextCopyLinks') }}</template>
    <template #toggle><EditIcon />{{ i18n('instance.contextToggleSelected') }}</template>
    <template #disable><XIcon />{{ i18n('instance.contextDisableSelected') }}</template>
    <template #enable><CheckCircleIcon />{{ i18n('instance.contextEnableSelected') }}</template>
    <template #hide_show><EyeIcon />{{ i18n('instance.contextShowHide') }}</template>
    <template #update_all
      ><UpdatedIcon />{{ i18n('instance.contextUpdateSelected', { count: selected.length > 0 ? i18n('instance.contextSelected') : i18n('instance.contextAll') }) }}</template
    >
    <template #filter_update><UpdatedIcon />{{ i18n('instance.contextSelectUpdatable') }}</template>
  </ContextMenu>
</template>
<script setup>
import { Button, Avatar, Card } from '@modrinth/ui'
import SlideRail from '@/components/ui/SlideRail.vue'
import { ServerIcon } from '@/assets/icons'
import {
  BoxIcon,
  SettingsIcon,
  FileIcon,
  PlayIcon,
  StopCircleIcon,
  EditIcon,
  FolderOpenIcon,
  ClipboardCopyIcon,
  PlusIcon,
  ExternalIcon,
  HashIcon,
  GlobeIcon,
  EyeIcon,
  XIcon,
  CheckCircleIcon,
  UpdatedIcon,
} from '@modrinth/assets'
import { invoke } from '@tauri-apps/api/core'
import { get, kill, run } from '@/helpers/profile'
import { get_by_profile_path } from '@/helpers/process'
import { process_listener, profile_listener } from '@/helpers/events'
import { useRoute, useRouter } from 'vue-router'
import { computed, ref, onUnmounted, onErrorCaptured, watch } from 'vue'
import { handleError, useBreadcrumbs, useLoading } from '@/store/state'
import { showProfileInFolder } from '@/helpers/utils.js'
import ContextMenu from '@/components/ui/ContextMenu.vue'
import { instanceIconSrc } from '@/helpers/instanceIcon.js'
import { t as i18n, locale } from '@/helpers/i18n.js'
import { instancePlaySeconds, playtimeLabel as formatPlayLabel } from '@/helpers/playtime.js'
import { get_project, get_version_many } from '@/helpers/cache.js'
import { handleSevereError } from '@/store/error.js'
import dayjs from 'dayjs'

const route = useRoute()

const router = useRouter()
const breadcrumbs = useBreadcrumbs()

const loaded = await get(route.params.id).catch(handleError)
const instance = ref(
  loaded && typeof loaded === 'object'
    ? loaded
    : {
        name: String(route.params.id || 'Instance'),
        path: route.params.id,
        loader: 'vanilla',
        game_version: '',
        install_stage: 'installed',
      },
)

watch(
  () => route.params.id,
  async (id) => {
    if (!id) return
    const next = await get(id).catch(handleError)
    if (next && typeof next === 'object') {
      instance.value = next
      breadcrumbs.setName(
        'Instance',
        next.name.length > 40 ? `${next.name.substring(0, 40)}...` : next.name,
      )
      breadcrumbs.setContext({ name: next.name, link: route.path, query: route.query })
      await checkProcess()
    }
  },
)

breadcrumbs.setName(
  'Instance',
  (instance.value?.name || 'Instance').length > 40
    ? (instance.value?.name || 'Instance').substring(0, 40) + '...'
    : instance.value?.name || 'Instance',
)

breadcrumbs.setContext({
  name: instance.value.name,
  link: route.path,
  query: route.query,
})

const offline = ref(!navigator.onLine)
window.addEventListener('offline', () => {
  offline.value = true
})
window.addEventListener('online', () => {
  offline.value = false
})

const loadingBar = useLoading()

const playing = ref(false)
const loading = ref(false)
const nativeRunning = ref(false)
let nativeChildId = null
const options = ref(null)
const selected = ref([])
const childError = ref('')
const playtimeLabel = computed(() => {
  void locale.value
  return formatPlayLabel(instancePlaySeconds(instance.value), i18n)
})

async function refreshInstance() {
  const next = await get(route.params.id).catch(handleError)
  if (next && typeof next === 'object') instance.value = next
}

let playtimeTimer = null
watch(playing, (isPlaying) => {
  if (playtimeTimer) {
    clearInterval(playtimeTimer)
    playtimeTimer = null
  }
  if (isPlaying) {
    playtimeTimer = setInterval(() => {
      refreshInstance()
    }, 10000)
  } else {
    refreshInstance()
  }
})

onErrorCaptured((err) => {
  childError.value = err?.message || String(err)
  return false
})

const startInstance = async (context, server) => {
  loading.value = true
  try {
    if (instance.value.native_client) {
      nativeChildId = await run(route.params.id, server || null)
      nativeRunning.value = true
      const waitForExit = async () => {
        if (!nativeChildId) return
        const exited = await invoke('native_client_exited', { pid: nativeChildId }).catch(() => true)
        if (exited) { nativeRunning.value = false; nativeChildId = null }
        else window.setTimeout(waitForExit, 1000)
      }
      window.setTimeout(waitForExit, 1000)
      return
    }
    await run(route.params.id, server || null)
    playing.value = true
  } catch (err) {
    handleSevereError(err, { profilePath: route.params.id })
  } finally {
    loading.value = false
  }
}

const checkProcess = async () => {
  if (instance.value.native_client) return
  const runningProcesses = await get_by_profile_path(route.params.id).catch(handleError)
  playing.value = runningProcesses.length > 0
}

// Get information on associated modrinth versions, if any
const modrinthVersions = ref([])
if (!offline.value && instance.value.linked_data && instance.value.linked_data.project_id) {
  get_project(instance.value.linked_data.project_id, 'must_revalidate')
    .catch(handleError)
    .then((project) => {
      if (project && project.versions) {
        get_version_many(project.versions, 'must_revalidate')
          .catch(handleError)
          .then((versions) => {
            modrinthVersions.value = versions.sort(
              (a, b) => dayjs(b.date_published) - dayjs(a.date_published),
            )
          })
      }
    })
}

await checkProcess()

const stopInstance = async (context) => {
  if (instance.value.native_client) {
    if (nativeChildId) await invoke('native_client_kill', { pid: nativeChildId }).catch(handleError)
    nativeChildId = null
    nativeRunning.value = false
    return
  }
  playing.value = false
  await kill(route.params.id).catch(handleError)
}

const handleRightClick = (event) => {
  const baseOptions = [
    { name: 'add_content' },
    { type: 'divider' },
    { name: 'edit' },
    { name: 'open_folder' },
    { name: 'copy_path' },
  ]

  options.value.showMenu(
    event,
    instance.value,
    playing.value
      ? [
          {
            name: 'stop',
            color: 'danger',
          },
          ...baseOptions,
        ]
      : [
          {
            name: 'play',
            color: 'primary',
          },
          ...baseOptions,
        ],
  )
}

const handleOptionsClick = async (args) => {
  switch (args.option) {
    case 'play':
      await startInstance('InstancePageContextMenu')
      break
    case 'stop':
      await stopInstance('InstancePageContextMenu')
      break
    case 'add_content':
      await router.push({
        path: `/browse/${instance.value.loader === 'vanilla' ? 'datapack' : 'mod'}`,
        query: { i: route.params.id },
      })
      break
    case 'edit':
      await router.push({
        path: `/instance/${encodeURIComponent(route.params.id)}/options`,
      })
      break
    case 'open_folder':
      await showProfileInFolder(instance.value.path)
      break
    case 'copy_path':
      await navigator.clipboard.writeText(instance.value.path)
      break
  }
}

function sameProfile(event) {
  const id = String(route.params.id || '')
  if (!id) return false
  return [event.profile_path_id, event.path, event.uuid, event.profile_path].includes(id)
}

const unlistenProfiles = await profile_listener(async (event) => {
  if (!sameProfile(event)) return
  if (event.event === 'removed' || event.event === 'Removed') {
    await router.push({ path: '/' })
    return
  }
  const next = await get(route.params.id).catch(handleError)
  if (next && typeof next === 'object') instance.value = next
})

const unlistenProcesses = await process_listener((e) => {
  const event = String(e?.event || '').toLowerCase()
  if (e.profile_path_id !== route.params.id) return
  if (event === 'finished' || event === 'crashed') playing.value = false
})

onUnmounted(() => {
  if (playtimeTimer) clearInterval(playtimeTimer)
  unlistenProcesses()
  unlistenProfiles()
})
</script>

<style scoped lang="scss">
.instance-card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.playtime {
  display: block;
  margin-top: 0.15rem;
  color: var(--color-secondary);
  font-size: 0.8rem;
  font-weight: 500;
  letter-spacing: 0.01em;
}

.button-group {
  display: flex;
  flex-direction: row;
  gap: 0.5rem;
}

.side-cards {
  position: fixed;
  width: 300px;
  display: flex;
  flex-direction: column;

  min-height: calc(100vh - 3.25rem);
  max-height: calc(100vh - 3.25rem);
  overflow-y: auto;
  -ms-overflow-style: none;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    width: 0;
    background: transparent;
  }

  .card {
    min-height: unset;
    margin-bottom: 0;
  }
}

.instance-nav {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  padding: 1rem;
  gap: 0.5rem;
  background: var(--color-raised-bg);
  height: 100%;
}

.name {
  font-size: 1.25rem;
  color: var(--color-contrast);
  overflow: hidden;
  text-overflow: ellipsis;
}

.metadata {
  text-transform: capitalize;
}

.instance-container {
  display: flex;
  flex-direction: row;
  overflow: auto;
  gap: 1rem;
  min-height: 100%;
  padding: 1rem;
}

.instance-info {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.badge {
  display: flex;
  align-items: center;
  font-weight: bold;
  width: fit-content;
  color: var(--color-orange);
}

.pages-list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);

  .btn {
    font-size: 100%;
    font-weight: 400;
    background: inherit;
    transition: color 0.28s ease, background-color 0.28s ease;
    width: 100%;
    color: var(--color-primary);
    box-shadow: none;

    &.router-link-exact-active {
      box-shadow: none;
      background: transparent;
      color: var(--color-contrast);
    }

    &:hover {
      background-color: var(--color-button-bg);
      color: var(--color-contrast);
      box-shadow: var(--shadow-inset-lg);
      text-decoration: none;
    }

    svg {
      width: 1.3rem;
      height: 1.3rem;
    }
  }
}

.instance-nav {
  display: flex;
  flex-direction: row;
  align-items: flex-start;
  justify-content: left;
  padding: 1rem;
  gap: 0.5rem;
  height: min-content;
  width: 100%;
}

.instance-button {
  width: fit-content;
}

.actions {
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  gap: 0.5rem;
}

.content {
  margin: 0 0 0.5rem 316px;
  width: calc(100% - 316px);
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: auto;
}

.child-error {
  color: var(--color-red);
}

.stats {
  grid-area: stats;
  display: flex;
  flex-direction: column;
  flex-wrap: wrap;
  gap: var(--gap-md);

  .stat {
    display: flex;
    flex-direction: row;
    align-items: center;
    width: fit-content;
    gap: var(--gap-xs);
    --stat-strong-size: 1.25rem;

    strong {
      font-size: var(--stat-strong-size);
    }

    p {
      margin: 0;
    }

    svg {
      height: var(--stat-strong-size);
      width: var(--stat-strong-size);
    }
  }

  .date {
    margin-top: auto;
  }

  @media screen and (max-width: 750px) {
    flex-direction: row;
    column-gap: var(--gap-md);
    margin-top: var(--gap-xs);
  }

  @media screen and (max-width: 600px) {
    margin-top: 0;

    .stat-label {
      display: none;
    }
  }
}
</style>

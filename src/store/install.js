import { defineStore } from 'pinia'
import { add_project_from_version, check_installed, list } from '@/helpers/profile.js'
import { handleError } from '@/store/notifications.js'
import { get_project, get_version_many } from '@/helpers/cache.js'
import { invoke } from '@tauri-apps/api/core'
import { t } from '@/helpers/i18n.js'
import { useNotifications } from '@/store/notifications.js'
import dayjs from 'dayjs'

export const useInstall = defineStore('installStore', {
  state: () => ({
    installConfirmModal: null,
    modInstallModal: null,
    incompatibilityWarningModal: null,
  }),
  actions: {
    setInstallConfirmModal(ref) {
      this.installConfirmModal = ref
    },
    showInstallConfirmModal(project, version_id, onInstall) {
      this.installConfirmModal.show(project, version_id, onInstall)
    },
    setIncompatibilityWarningModal(ref) {
      this.incompatibilityWarningModal = ref
    },
    showIncompatibilityWarningModal(instance, project, versions, onInstall) {
      this.incompatibilityWarningModal.show(instance, project, versions, onInstall)
    },
    setModInstallModal(ref) {
      this.modInstallModal = ref
    },
    showModInstallModal(project, versions, onInstall) {
      this.modInstallModal.show(project, versions, onInstall)
    },
  },
})

async function resolveInstancePath(instancePath) {
  if (instancePath) return instancePath
  const packs = (await list().catch(handleError)) ?? []
  return packs[0]?.path ?? null
}

function detectCatalog(projectId, source, projectHint = {}) {
  const hint = String(projectHint.source || '').toLowerCase()
  const passed = String(source || '').toLowerCase()
  const id = String(projectId || '')
  if (hint.includes('github') || passed.includes('github') || /github\.com/i.test(id)) {
    return 'github'
  }
  if (
    hint.includes('curse') ||
    passed.includes('curse') ||
    id.startsWith('cf-') ||
    /^\d+$/.test(id.replace(/^cf-/i, ''))
  ) {
    return 'curseforge'
  }
  return 'modrinth'
}

function normalizeVersions(rows) {
  if (!Array.isArray(rows)) return []
  return rows.map((row) => ({
    ...row,
    id: String(row.id ?? row.fileId ?? ''),
    game_versions: row.game_versions || row.gameVersions || [],
    loaders: row.loaders || [],
    name: row.name || row.version_number || row.versionNumber || String(row.id ?? ''),
    dependencies: row.dependencies || [],
  }))
}

async function fetchProjectVersions(projectId, catalog) {
  const rows = await invoke('list_project_versions', {
    id: String(projectId),
    source: catalog,
    gameVersion: '',
    loader: '',
  }).catch(() => null)
  if (Array.isArray(rows) && rows.length) return normalizeVersions(rows)

  // Fallback: unfiltered Modrinth version list.
  if (catalog === 'modrinth') {
    const all = await invoke('catalog_get_json', {
      url: `https://api.modrinth.com/v2/project/${encodeURIComponent(String(projectId))}/version`,
    }).catch(() => [])
    return normalizeVersions(all)
  }
  return []
}

export const install = async (
  projectId,
  versionId,
  instancePath,
  source,
  callback = () => {},
  projectHint = {},
) => {
  const catalog = detectCatalog(projectId, source, projectHint)
  const projectType = projectHint.project_type || projectHint.projectType || 'mod'
  if (!instancePath) {
    // Modpacks become a new profile with the pack's icon.
    if (String(projectType).toLowerCase() === 'modpack') {
      try {
        const { install: packInstall } = await import('@/helpers/pack.js')
        const title = projectHint.title || projectHint.name || projectId
        const icon = projectHint.icon_url || projectHint.iconUrl || null
        const path = await packInstall(projectId, versionId, title, icon)
        try {
          const notifs = useNotifications()
          notifs.addNotification({
            title: t('install.savedTitle'),
            text: path || title,
            type: 'success',
          })
        } catch {
          /* ignore */
        }
        callback(versionId || 'latest')
      } catch (error) {
        handleError(error)
      }
      return
    }

    // Mods / resource packs: pick a profile instead of dumping into Downloads.
    try {
      let project = projectHint
      if (!project?.title && !project?.name) {
        project = (await get_project(projectId).catch(() => null)) || projectHint
      }
      project = {
        ...project,
        id: project.id || project.project_id || projectId,
        project_id: project.project_id || project.id || projectId,
        project_type: projectType,
        source: catalog,
      }
      const versions = versionId
        ? normalizeVersions([
            (await invoke('get_catalog_version', { id: String(versionId) }).catch(() => null)) || {
              id: versionId,
              game_versions: project.game_versions || [],
              loaders: project.loaders || [],
              dependencies: [],
            },
          ])
        : await fetchProjectVersions(projectId, catalog)
      if (!versions.length) {
        handleError(new Error(t('install.noVersions')))
        return
      }
      const store = useInstall()
      if (store.modInstallModal) {
        store.showModInstallModal(project, versions, callback)
        return
      }
      // Fallback if modal not mounted yet: install into first profile.
      const target = await resolveInstancePath(null)
      if (!target) {
        handleError(new Error(t('install.needProfile')))
        return
      }
      await invoke('install_content', {
        request: {
          instanceId: target,
          projectId: String(projectId),
          versionId: versions[0].id || versionId || null,
          source: catalog,
          projectType,
        },
      })
      callback(versions[0].id || versionId || 'latest')
    } catch (error) {
      handleError(error)
    }
    return
  }

  const target = await resolveInstancePath(instancePath)
  if (!target) {
    handleError(new Error(t('install.needProfile')))
    return
  }
  try {
    await invoke('install_content', {
      request: {
        instanceId: target,
        projectId: String(projectId),
        versionId: versionId || null,
        source: catalog,
        projectType,
      },
    })
    callback(versionId || 'latest')
  } catch (error) {
    handleError(error)
  }
}

export const installVersionDependencies = async (profile, version) => {
  for (const dep of version.dependencies || []) {
    if (dep.dependency_type !== 'required') continue
    if (dep.project_id === 'P7dR8mSH' && profile.loader === 'quilt') continue
    if (dep.version_id) {
      if (dep.project_id && (await check_installed(profile.path, dep.project_id).catch(handleError)))
        continue
      await add_project_from_version(profile.path, dep.version_id, {
        projectId: dep.project_id,
        source: 'modrinth',
      })
    } else {
      if (dep.project_id && (await check_installed(profile.path, dep.project_id).catch(handleError)))
        continue

      const depProject = await get_project(dep.project_id, 'must_revalidate').catch(handleError)
      if (!depProject) continue
      const versionIds = depProject.versions || []
      const depVersions = (
        await get_version_many(versionIds, 'must_revalidate').catch(handleError)
      ).sort((a, b) => dayjs(b.date_published) - dayjs(a.date_published))

      const latest = depVersions.find(
        (v) =>
          (v.game_versions || []).includes(profile.game_version) &&
          (v.loaders || []).includes(profile.loader),
      )
      if (latest) {
        await add_project_from_version(profile.path, latest.id, {
          projectId: dep.project_id,
          source: 'modrinth',
        }).catch(handleError)
      }
    }
  }
}

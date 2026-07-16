<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';
  import { locale, t } from '$lib/i18n/index.js';
  import { showToast } from '$lib/stores/toast.js';

  export let config;

  const dispatch = createEventDispatcher();

  let loading = true;
  let saving = false;
  let rules = [];
  let loadError = '';
  let importSourceDir = '';
  let importPreview = null;
  let importPreviewing = false;
  let importing = false;
  $: currentLocale = $locale;

  function normalizeRule(rule = {}) {
    return {
      project_key: rule.project_key || '',
      project_name: rule.project_name || '',
      obsidian_page: rule.obsidian_page || '',
      local_paths: Array.isArray(rule.local_paths) ? rule.local_paths : [],
      domains: Array.isArray(rule.domains) ? rule.domains : [],
      url_keywords: Array.isArray(rule.url_keywords) ? rule.url_keywords : [],
      window_keywords: Array.isArray(rule.window_keywords) ? rule.window_keywords : [],
      app_keywords: Array.isArray(rule.app_keywords) ? rule.app_keywords : [],
      negative_keywords: Array.isArray(rule.negative_keywords) ? rule.negative_keywords : [],
      priority: Number.isFinite(rule.priority) ? rule.priority : 0,
    };
  }

  function normalizeConfigRules() {
    if (!Array.isArray(config.work_journal_project_rules)) {
      config.work_journal_project_rules = [];
    }
    config.work_journal_project_rules = config.work_journal_project_rules.map(normalizeRule);
  }

  function normalizeObsidianSettings() {
    if (!config.work_journal_obsidian || typeof config.work_journal_obsidian !== 'object') {
      config.work_journal_obsidian = {};
    }
    config.work_journal_obsidian = {
      vault_path: config.work_journal_obsidian.vault_path || '',
      daily_folder: config.work_journal_obsidian.daily_folder || 'Work Journal',
      export_mode: config.work_journal_obsidian.export_mode || 'preview_only',
      conflict_behavior: config.work_journal_obsidian.conflict_behavior || 'append_under_marker',
    };
  }

  function normalizeAiSettings() {
    if (!config.work_journal_ai || typeof config.work_journal_ai !== 'object') {
      config.work_journal_ai = {};
    }
    config.work_journal_ai = {
      enabled: Boolean(config.work_journal_ai.enabled),
      vision_enabled: Boolean(config.work_journal_ai.vision_enabled),
      confidence_threshold: Number(config.work_journal_ai.confidence_threshold) || 80,
      max_sessions_per_run: Number(config.work_journal_ai.max_sessions_per_run) || 5,
    };
  }

  function updateObsidianSetting(field, value) {
    config.work_journal_obsidian = {
      ...config.work_journal_obsidian,
      [field]: value,
    };
    dispatch('change', config);
  }

  function updateAiSetting(field, value) {
    const next = {
      ...config.work_journal_ai,
      [field]: value,
    };
    if (field === 'enabled' && !value) next.vision_enabled = false;
    config.work_journal_ai = next;
    dispatch('change', config);
  }

  async function chooseVault() {
    const selected = await openDialog({ directory: true, multiple: false });
    if (selected && !Array.isArray(selected)) {
      updateObsidianSetting('vault_path', selected);
    }
  }

  async function chooseImportSource() {
    const selected = await openDialog({ directory: true, multiple: false });
    if (selected && !Array.isArray(selected)) {
      importSourceDir = selected;
      importPreview = null;
    }
  }

  async function previewLegacyImport() {
    importPreviewing = true;
    importPreview = null;
    try {
      importPreview = await invoke('preview_work_review_import', {
        sourceDir: importSourceDir.trim() || null,
      });
      importSourceDir = importPreview.source_dir;
    } catch (error) {
      showToast(t('settingsWorkJournal.importPreviewFailed', { error }), 'error');
    } finally {
      importPreviewing = false;
    }
  }

  async function importLegacyData() {
    if (!importPreview?.confirmation_token || importing) return;
    const confirmed = await ask(t('settingsWorkJournal.importConfirmMessage', {
      count: importPreview.activity_count,
    }), {
      title: t('settingsWorkJournal.importConfirmTitle'),
      kind: 'warning',
    });
    if (!confirmed) return;

    importing = true;
    try {
      const result = await invoke('import_work_review_data', {
        input: { confirmation_token: importPreview.confirmation_token },
      });
      showToast(t('settingsWorkJournal.importSuccess', {
        imported: result.imported_count,
        duplicates: result.skipped_duplicate_count,
      }), 'success');
      importPreview = null;
    } catch (error) {
      importPreview = null;
      showToast(t('settingsWorkJournal.importFailed', { error }), 'error');
    } finally {
      importing = false;
    }
  }

  function formatImportBytes(bytes) {
    const value = Number(bytes) || 0;
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
    return `${(value / 1024 / 1024).toFixed(1)} MB`;
  }

  function toList(value) {
    return Array.isArray(value) ? value.join(', ') : '';
  }

  function fromList(value) {
    return value
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  function updateRule(index, field, value) {
    const nextRules = [...rules];
    const nextRule = { ...nextRules[index] };
    if (field === 'priority') {
      nextRule.priority = Number.parseInt(value, 10) || 0;
    } else if (Array.isArray(nextRule[field])) {
      nextRule[field] = fromList(value);
    } else {
      nextRule[field] = value;
    }
    nextRules[index] = nextRule;
    rules = nextRules;
    config.work_journal_project_rules = rules;
    dispatch('change', config);
  }

  function addRule() {
    rules = [
      ...rules,
      normalizeRule({
        project_key: `project-${rules.length + 1}`,
        project_name: t('settingsWorkJournal.newProjectName'),
        obsidian_page: '',
        priority: 10,
      }),
    ];
    config.work_journal_project_rules = rules;
    dispatch('change', config);
  }

  function removeRule(index) {
    rules = rules.filter((_, ruleIndex) => ruleIndex !== index);
    config.work_journal_project_rules = rules;
    dispatch('change', config);
  }

  async function loadRules() {
    loading = true;
    loadError = '';
    try {
      normalizeObsidianSettings();
      normalizeAiSettings();
      const loadedRules = await invoke('get_work_journal_project_rules');
      rules = loadedRules.map(normalizeRule);
      config.work_journal_project_rules = rules;
    } catch (error) {
      loadError = error?.toString?.() || String(error);
      normalizeConfigRules();
      rules = config.work_journal_project_rules;
    } finally {
      loading = false;
    }
  }

  async function saveRules() {
    saving = true;
    try {
      const [savedRules, savedObsidianSettings, savedAiSettings] = await Promise.all([
        invoke('save_work_journal_project_rules', { rules }),
        invoke('save_work_journal_obsidian_settings', {
          settings: config.work_journal_obsidian,
        }),
        invoke('save_work_journal_ai_settings', {
          settings: config.work_journal_ai,
        }),
      ]);
      rules = savedRules.map(normalizeRule);
      config.work_journal_project_rules = rules;
      config.work_journal_obsidian = savedObsidianSettings;
      config.work_journal_ai = savedAiSettings;
      showToast(t('settingsWorkJournal.saveSuccess'), 'success');
      dispatch('change', { autosaved: true });
    } catch (error) {
      showToast(t('settingsWorkJournal.saveFailed', { error }), 'error');
    } finally {
      saving = false;
    }
  }

  onMount(loadRules);
</script>

<div class="settings-card work-journal-settings-shell" data-locale={currentLocale}>
  <div class="flex items-center justify-between gap-3">
    <div>
      <h3 class="settings-card-title mb-0">{t('settingsWorkJournal.title')}</h3>
      <p class="settings-card-desc mb-0">{t('settingsWorkJournal.description')}</p>
    </div>
    <div class="settings-actions">
      <button type="button" class="settings-action-secondary" on:click={addRule}>
        {t('settingsWorkJournal.addRule')}
      </button>
      <button type="button" class="settings-action-primary" disabled={saving || loading} on:click={saveRules}>
        {saving ? t('settingsWorkJournal.saving') : t('settingsWorkJournal.saveRules')}
      </button>
    </div>
  </div>

  {#if loadError}
    <div class="page-banner-error mt-4">
      <p>{t('settingsWorkJournal.loadFailed', { error: loadError })}</p>
      <button type="button" class="page-action-brand" on:click={loadRules}>{t('settings.retry')}</button>
    </div>
  {/if}

  <section class="settings-panel work-journal-export-settings mt-4">
    <div class="work-journal-export-head">
      <div>
        <div class="settings-text font-semibold">{t('settingsWorkJournal.exportTitle')}</div>
        <div class="settings-subtle">{t('settingsWorkJournal.exportDescription')}</div>
      </div>
    </div>
    <div class="work-journal-rule-grid">
      <label class="settings-field work-journal-span-2">
        <span class="settings-label">{t('settingsWorkJournal.vaultPath')}</span>
        <div class="work-journal-path-control">
          <input
            class="control-input"
            value={config.work_journal_obsidian.vault_path}
            on:input={(event) => updateObsidianSetting('vault_path', event.currentTarget.value)}
          />
          <button type="button" class="settings-action-secondary" on:click={chooseVault}>
            {t('settingsWorkJournal.chooseVault')}
          </button>
        </div>
      </label>
      <label class="settings-field">
        <span class="settings-label">{t('settingsWorkJournal.dailyFolder')}</span>
        <input
          class="control-input"
          value={config.work_journal_obsidian.daily_folder}
          on:input={(event) => updateObsidianSetting('daily_folder', event.currentTarget.value)}
        />
      </label>
      <label class="settings-field">
        <span class="settings-label">{t('settingsWorkJournal.exportMode')}</span>
        <select
          class="control-input"
          value={config.work_journal_obsidian.export_mode}
          on:change={(event) => updateObsidianSetting('export_mode', event.currentTarget.value)}
        >
          <option value="preview_only">{t('settingsWorkJournal.previewOnly')}</option>
          <option value="daily_log">{t('settingsWorkJournal.dailyLog')}</option>
        </select>
      </label>
      <label class="settings-field">
        <span class="settings-label">{t('settingsWorkJournal.conflictBehavior')}</span>
        <select
          class="control-input"
          value={config.work_journal_obsidian.conflict_behavior}
          on:change={(event) => updateObsidianSetting('conflict_behavior', event.currentTarget.value)}
        >
          <option value="append_under_marker">{t('settingsWorkJournal.replaceMarker')}</option>
          <option value="create_new">{t('settingsWorkJournal.createNew')}</option>
          <option value="manual_copy">{t('settingsWorkJournal.manualCopy')}</option>
        </select>
      </label>
    </div>
  </section>

  <section class="settings-panel work-journal-import-settings mt-4">
    <div class="work-journal-export-head">
      <div>
        <div class="settings-text font-semibold">{t('settingsWorkJournal.importTitle')}</div>
        <div class="settings-subtle">{t('settingsWorkJournal.importDescription')}</div>
      </div>
    </div>
    <div class="work-journal-path-control work-journal-import-path-control mt-3">
      <input
        class="control-input"
        value={importSourceDir}
        placeholder={t('settingsWorkJournal.importDefaultPath')}
        on:input={(event) => {
          importSourceDir = event.currentTarget.value;
          importPreview = null;
        }}
      />
      <button type="button" class="settings-action-secondary" on:click={chooseImportSource}>
        {t('settingsWorkJournal.chooseImportSource')}
      </button>
      <button
        type="button"
        class="settings-action-secondary"
        disabled={importPreviewing || importing}
        on:click={previewLegacyImport}
      >
        {importPreviewing ? t('settingsWorkJournal.importPreviewing') : t('settingsWorkJournal.importPreview')}
      </button>
    </div>
    {#if importPreview}
      <div class="work-journal-import-summary mt-3" data-testid="work-review-import-preview">
        <span>{t('settingsWorkJournal.importActivities', { count: importPreview.activity_count })}</span>
        <span>{t('settingsWorkJournal.importDateRange', {
          from: importPreview.date_from || '-',
          to: importPreview.date_to || '-',
        })}</span>
        <span>{t('settingsWorkJournal.importScreenshots', {
          count: importPreview.screenshot_count,
          size: formatImportBytes(importPreview.screenshot_bytes),
        })}</span>
        {#if importPreview.skipped_screenshot_count}
          <span>{t('settingsWorkJournal.importSkippedScreenshots', {
            count: importPreview.skipped_screenshot_count,
          })}</span>
        {/if}
        <button
          type="button"
          class="settings-action-primary"
          disabled={importing}
          on:click={importLegacyData}
        >
          {importing ? t('settingsWorkJournal.importing') : t('settingsWorkJournal.importConfirm')}
        </button>
      </div>
    {/if}
  </section>

  <section class="settings-panel work-journal-ai-settings mt-4">
    <div class="work-journal-export-head">
      <div>
        <div class="settings-text font-semibold">{t('settingsWorkJournal.aiTitle')}</div>
        <div class="settings-subtle">{t('settingsWorkJournal.aiPrivacy')}</div>
      </div>
    </div>
    <div class="work-journal-ai-toggles">
      <label class="work-journal-toggle-row">
        <span>
          <strong>{t('settingsWorkJournal.aiEnabled')}</strong>
          <small>{t('settingsWorkJournal.aiEnabledMeta')}</small>
        </span>
        <input
          type="checkbox"
          class="accent-primary-500"
          checked={config.work_journal_ai.enabled}
          on:change={(event) => updateAiSetting('enabled', event.currentTarget.checked)}
        />
      </label>
      <label class="work-journal-toggle-row">
        <span>
          <strong>{t('settingsWorkJournal.visionEnabled')}</strong>
          <small>{t('settingsWorkJournal.visionEnabledMeta')}</small>
        </span>
        <input
          type="checkbox"
          class="accent-primary-500"
          checked={config.work_journal_ai.vision_enabled}
          disabled={!config.work_journal_ai.enabled}
          on:change={(event) => updateAiSetting('vision_enabled', event.currentTarget.checked)}
        />
      </label>
    </div>
    <div class="work-journal-rule-grid">
      <label class="settings-field">
        <span class="settings-label">{t('settingsWorkJournal.confidenceThreshold')}</span>
        <input
          class="control-input"
          type="number"
          min="1"
          max="100"
          value={config.work_journal_ai.confidence_threshold}
          on:input={(event) => updateAiSetting('confidence_threshold', Number(event.currentTarget.value))}
        />
      </label>
      <label class="settings-field">
        <span class="settings-label">{t('settingsWorkJournal.maxSessions')}</span>
        <input
          class="control-input"
          type="number"
          min="1"
          max="20"
          value={config.work_journal_ai.max_sessions_per_run}
          on:input={(event) => updateAiSetting('max_sessions_per_run', Number(event.currentTarget.value))}
        />
      </label>
    </div>
  </section>

  {#if loading}
    <div class="settings-panel mt-4">
      <span class="settings-muted">{t('settingsWorkJournal.loading')}</span>
    </div>
  {:else if rules.length === 0}
    <div class="settings-panel mt-4">
      <span class="settings-muted">{t('settingsWorkJournal.empty')}</span>
    </div>
  {:else}
    <div class="work-journal-rule-list mt-4">
      {#each rules as rule, index}
        <section class="settings-panel work-journal-rule">
          <div class="work-journal-rule-head">
            <div>
              <div class="settings-text font-semibold">{rule.project_name || rule.project_key}</div>
              <div class="settings-subtle">{rule.project_key}</div>
            </div>
            <button type="button" class="settings-link-action" on:click={() => removeRule(index)}>
              {t('settingsWorkJournal.removeRule')}
            </button>
          </div>

          <div class="work-journal-rule-grid">
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.projectName')}</span>
              <input class="control-input" value={rule.project_name} on:input={(event) => updateRule(index, 'project_name', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.projectKey')}</span>
              <input class="control-input" value={rule.project_key} on:input={(event) => updateRule(index, 'project_key', event.currentTarget.value)} />
            </label>
            <label class="settings-field work-journal-span-2">
              <span class="settings-label">{t('settingsWorkJournal.obsidianPage')}</span>
              <input class="control-input" value={rule.obsidian_page} on:input={(event) => updateRule(index, 'obsidian_page', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.domains')}</span>
              <input class="control-input" value={toList(rule.domains)} on:input={(event) => updateRule(index, 'domains', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.localPaths')}</span>
              <input class="control-input" value={toList(rule.local_paths)} on:input={(event) => updateRule(index, 'local_paths', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.windowKeywords')}</span>
              <input class="control-input" value={toList(rule.window_keywords)} on:input={(event) => updateRule(index, 'window_keywords', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.urlKeywords')}</span>
              <input class="control-input" value={toList(rule.url_keywords)} on:input={(event) => updateRule(index, 'url_keywords', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.appKeywords')}</span>
              <input class="control-input" value={toList(rule.app_keywords)} on:input={(event) => updateRule(index, 'app_keywords', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.negativeKeywords')}</span>
              <input class="control-input" value={toList(rule.negative_keywords)} on:input={(event) => updateRule(index, 'negative_keywords', event.currentTarget.value)} />
            </label>
            <label class="settings-field">
              <span class="settings-label">{t('settingsWorkJournal.priority')}</span>
              <input class="control-input" type="number" value={rule.priority} on:input={(event) => updateRule(index, 'priority', event.currentTarget.value)} />
            </label>
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .work-journal-rule-list {
    display: grid;
    gap: 12px;
  }

  .work-journal-export-settings {
    display: grid;
    gap: 14px;
  }

  .work-journal-ai-settings {
    display: grid;
    gap: 14px;
  }

  .work-journal-import-settings {
    display: grid;
    gap: 14px;
  }

  .work-journal-import-path-control {
    grid-template-columns: minmax(0, 1fr) auto auto;
  }

  .work-journal-import-summary {
    display: flex;
    align-items: center;
    gap: 10px 16px;
    flex-wrap: wrap;
    color: var(--text-secondary);
    font-size: 0.82rem;
    letter-spacing: 0;
  }

  .work-journal-import-summary .settings-action-primary {
    margin-left: auto;
  }

  .work-journal-ai-toggles {
    display: grid;
    gap: 8px;
  }

  .work-journal-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 44px;
  }

  .work-journal-toggle-row span {
    display: grid;
    gap: 2px;
  }

  .work-journal-toggle-row strong,
  .work-journal-toggle-row small {
    letter-spacing: 0;
  }

  .work-journal-toggle-row small {
    color: var(--text-muted);
  }

  .work-journal-export-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .work-journal-path-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px;
  }

  .work-journal-rule {
    display: grid;
    gap: 14px;
  }

  .work-journal-rule-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .work-journal-rule-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .work-journal-span-2 {
    grid-column: span 2;
  }

  @media (max-width: 760px) {
    .work-journal-rule-grid {
      grid-template-columns: 1fr;
    }

    .work-journal-span-2 {
      grid-column: auto;
    }

    .work-journal-path-control {
      grid-template-columns: 1fr;
    }

    .work-journal-import-summary .settings-action-primary {
      width: 100%;
      margin-left: 0;
    }
  }
</style>

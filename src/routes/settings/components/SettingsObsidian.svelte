<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { locale, t } from '$lib/i18n/index.js';
  import { showToast } from '$lib/stores/toast.js';

  export let config;

  const dispatch = createEventDispatcher();

  let loading = true;
  let saving = false;
  let rules = [];
  let loadError = '';
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
      const savedRules = await invoke('save_work_journal_project_rules', { rules });
      rules = savedRules.map(normalizeRule);
      config.work_journal_project_rules = rules;
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
  }
</style>

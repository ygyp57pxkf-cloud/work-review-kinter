<script>
  import { invoke } from '@tauri-apps/api/core';
  import { ask } from '@tauri-apps/plugin-dialog';
  import { formatDurationLocalized, formatLocalizedTime, locale, t } from '$lib/i18n/index.js';
  import { showToast } from '../../lib/stores/toast.js';
  import LocalizedDatePicker from '../../lib/components/LocalizedDatePicker.svelte';

  function getLocalDateString() {
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  }

  let selectedDate = getLocalDateString();
  let journalDay = null;
  let previewMarkdown = '';
  let previewConfirmation = null;
  let loading = true;
  let previewing = false;
  let loadError = '';
  let previewError = '';
  let lastLoadedDate = '';
  let requestId = 0;
  let projectRules = [];
  let draftProjectKeys = {};
  let draftSummaries = {};
  let reviewingSessionId = null;
  let exporting = false;
  let obsidianWriteEnabled = false;
  let obsidianManualCopyEnabled = false;
  let aiEnabled = false;
  let aiRunning = false;
  $: currentLocale = $locale;

  function formatSessionTime(timestamp) {
    if (!Number.isFinite(timestamp)) return '--:--';
    return formatLocalizedTime(new Date(timestamp * 1000), {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
  }

  async function loadJournal() {
    const currentRequestId = ++requestId;
    loading = true;
    loadError = '';
    previewError = '';
    previewMarkdown = '';
    previewConfirmation = null;

    try {
      const [day, rules, config] = await Promise.all([
        invoke('get_work_journal_day', { date: selectedDate }),
        invoke('get_work_journal_project_rules'),
        invoke('get_config'),
      ]);
      if (currentRequestId === requestId) {
        journalDay = day;
        projectRules = rules;
        const obsidianSettings = config?.work_journal_obsidian || {};
        obsidianManualCopyEnabled = obsidianSettings.conflict_behavior === 'manual_copy';
        obsidianWriteEnabled = obsidianSettings.export_mode === 'daily_log'
          && obsidianSettings.conflict_behavior !== 'manual_copy'
          && Boolean(obsidianSettings.vault_path);
        aiEnabled = Boolean(config?.work_journal_ai?.enabled);
        initializeDrafts(day);
      }
    } catch (error) {
      if (currentRequestId === requestId) {
        journalDay = null;
        loadError = t('journal.loadError', { error: error?.toString?.() || String(error) });
      }
    } finally {
      if (currentRequestId === requestId) loading = false;
    }
  }

  function initializeDrafts(day) {
    draftProjectKeys = Object.fromEntries(
      (day?.sessions || []).map((session) => [session.id, session.review_state === 'included' ? session.project_key : '']),
    );
    draftSummaries = Object.fromEntries(
      (day?.sessions || []).map((session) => [session.id, session.task_summary || '']),
    );
  }

  function updateDraftProject(sessionId, projectKey) {
    draftProjectKeys = { ...draftProjectKeys, [sessionId]: projectKey };
  }

  function updateDraftSummary(sessionId, summary) {
    draftSummaries = { ...draftSummaries, [sessionId]: summary };
  }

  async function reviewSession(session, reviewState) {
    const selectedProjectKey = draftProjectKeys[session.id] || session.project_key;
    const project = projectRules.find((rule) => rule.project_key === selectedProjectKey);
    if (reviewState === 'included' && !project) return;

    reviewingSessionId = session.id;
    try {
      journalDay = await invoke('review_work_journal_session', {
        input: {
          session_id: session.id,
          date: selectedDate,
          project_key: project?.project_key || session.project_key,
          project_name: project?.project_name || session.project_name,
          obsidian_page: project?.obsidian_page || session.obsidian_page,
          task_summary: draftSummaries[session.id] || session.task_summary,
          review_state: reviewState,
        },
      });
      initializeDrafts(journalDay);
      previewMarkdown = '';
      previewConfirmation = null;
      showToast(t('journal.reviewSuccess'), 'success');
    } catch (error) {
      showToast(t('journal.reviewError', { error: error?.toString?.() || String(error) }), 'error');
    } finally {
      reviewingSessionId = null;
    }
  }

  async function previewExport() {
    previewing = true;
    previewError = '';
    try {
      const preview = await invoke('preview_work_journal_obsidian_export', { date: selectedDate });
      previewMarkdown = preview.markdown;
      previewConfirmation = {
        date: preview.date,
        content_hash: preview.content_hash,
        confirmation_token: preview.confirmation_token,
      };
    } catch (error) {
      previewMarkdown = '';
      previewConfirmation = null;
      previewError = t('journal.previewError', { error: error?.toString?.() || String(error) });
    } finally {
      previewing = false;
    }
  }

  async function analyzeWithAi() {
    if (!aiEnabled || aiRunning || !journalDay?.needs_review_count) return;
    aiRunning = true;
    try {
      const result = await invoke('analyze_work_journal_with_ai', { date: selectedDate });
      journalDay = result.day;
      initializeDrafts(journalDay);
      previewMarkdown = '';
      previewConfirmation = null;
      const message = t('journal.aiSuccess', {
        updated: result.updated_sessions,
        vision: result.vision_sessions,
      });
      showToast(message, result.errors?.length ? 'warning' : 'success');
    } catch (error) {
      showToast(t('journal.aiError', { error: error?.toString?.() || String(error) }), 'error');
    } finally {
      aiRunning = false;
    }
  }

  async function exportToObsidian() {
    if (!previewMarkdown || !previewConfirmation || !obsidianWriteEnabled) return;
    const confirmed = await ask(t('journal.exportConfirmMessage'), {
      title: t('journal.confirmExport'),
      kind: 'warning',
    });
    if (!confirmed) return;

    exporting = true;
    try {
      const result = await invoke('export_work_journal_obsidian', { input: previewConfirmation });
      showToast(t('journal.exportSuccess', { path: result.target_path }), 'success');
    } catch (error) {
      showToast(t('journal.exportError', { error: error?.toString?.() || String(error) }), 'error');
    } finally {
      exporting = false;
      previewConfirmation = null;
    }
  }

  async function copyPreviewMarkdown() {
    if (!previewMarkdown || !obsidianManualCopyEnabled) return;
    try {
      await navigator.clipboard.writeText(previewMarkdown);
      showToast(t('journal.copySuccess'), 'success');
    } catch (error) {
      showToast(t('journal.copyError', { error: error?.toString?.() || String(error) }), 'error');
    }
  }

  $: if (selectedDate && selectedDate !== lastLoadedDate) {
    lastLoadedDate = selectedDate;
    loadJournal();
  }
</script>

<div class="page-shell journal-page-shell" data-locale={currentLocale}>
  <div class="page-header">
    <div class="page-title-group">
      <div class="page-title-badge journal-title-badge" aria-hidden="true">
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M5 5h14M5 10h14M5 15h8M5 20h6" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m16 16 2 2 3-4" />
        </svg>
      </div>
      <div class="page-title-copy">
        <h2>{t('journal.title')}</h2>
        <p>{t('journal.sessionReview')}</p>
      </div>
    </div>

    <div class="page-toolbar journal-toolbar">
      {#key `journal-date-${currentLocale}`}
        <LocalizedDatePicker
          bind:value={selectedDate}
          max={getLocalDateString()}
          localeCode={currentLocale}
          triggerClass="page-control-input w-auto"
        />
      {/key}
      <button type="button" class="page-control-btn-icon" title={t('journal.refresh')} aria-label={t('journal.refresh')} disabled={loading} on:click={loadJournal}>
        <svg class:journal-spin={loading} class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M20 12a8 8 0 1 1-2.34-5.66M20 4v6h-6" />
        </svg>
      </button>
      <button
        type="button"
        class="page-control-btn"
        title={aiEnabled ? t('journal.analyzeWithAi') : t('journal.aiDisabled')}
        disabled={loading || aiRunning || !aiEnabled || !journalDay?.needs_review_count}
        on:click={analyzeWithAi}
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m12 3 1.4 4.1L17.5 8.5l-4.1 1.4L12 14l-1.4-4.1-4.1-1.4 4.1-1.4L12 3Z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m18 14 .8 2.2L21 17l-2.2.8L18 20l-.8-2.2L15 17l2.2-.8L18 14Z" />
        </svg>
        {aiRunning ? t('journal.aiRunning') : t('journal.analyzeWithAi')}
      </button>
      <button type="button" class="page-action-brand" disabled={loading || previewing || !journalDay} on:click={previewExport}>
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M12 3v12m0 0 4-4m-4 4-4-4M5 19h14" />
        </svg>
        {previewing ? t('journal.previewing') : t('journal.previewExport')}
      </button>
    </div>
  </div>

  {#if loadError}
    <div class="page-banner-error journal-error" role="alert">
      <p>{loadError}</p>
      <button type="button" class="page-action-brand" on:click={loadJournal}>{t('journal.refresh')}</button>
    </div>
  {:else if loading}
    <div class="journal-loading page-card-soft" aria-live="polite">
      <span class="journal-loader"></span>
    </div>
  {:else if journalDay}
    <section class="journal-metrics" aria-label={t('journal.title')}>
      <div class="journal-metric">
        <span>{t('journal.totalDuration')}</span>
        <strong>{formatDurationLocalized(journalDay.total_duration)}</strong>
      </div>
      <div class="journal-metric">
        <span>{t('journal.projects')}</span>
        <strong>{journalDay.project_count}</strong>
      </div>
      <div class="journal-metric">
        <span>{t('journal.sessions')}</span>
        <strong>{journalDay.sessions?.length || 0}</strong>
      </div>
      <div class="journal-metric journal-metric-review">
        <span>{t('journal.needsReview')}</span>
        <strong>{journalDay.needs_review_count}</strong>
      </div>
      <div class="journal-metric">
        <span>{t('journal.interruptions')}</span>
        <strong>{journalDay.interruption_count}</strong>
      </div>
    </section>

    <div class="journal-workspace">
      <div class="journal-main-column">
        <section class="journal-section">
          <div class="journal-section-head">
            <h3>{t('journal.projectOverview')}</h3>
          </div>
          {#if journalDay.project_summaries?.length}
            <div class="journal-project-list">
              {#each journalDay.project_summaries as project (project.project_key)}
                <article class="journal-project-row">
                  <div class="journal-project-name">
                    <span class:journal-project-dot-review={project.needs_review_count > 0} class="journal-project-dot"></span>
                    <div>
                      <strong>{project.project_name || t('journal.unassigned')}</strong>
                      <span>{project.session_count} {t('journal.sessions')}</span>
                    </div>
                  </div>
                  <div class="journal-project-duration">{formatDurationLocalized(project.duration, { compact: true })}</div>
                </article>
              {/each}
            </div>
          {:else}
            <div class="journal-empty">{t('journal.empty')}</div>
          {/if}
        </section>

        <section class="journal-section">
          <div class="journal-section-head">
            <h3>{t('journal.sessionReview')}</h3>
            <span>{journalDay.sessions?.length || 0}</span>
          </div>
          {#if journalDay.sessions?.length}
            <div class="journal-session-list">
              {#each journalDay.sessions as session (session.id)}
                <article class:journal-session-review={session.needs_review} class="journal-session-row">
                  <div class="journal-session-time">
                    <strong>{formatSessionTime(session.started_at)}</strong>
                    <span>{formatSessionTime(session.ended_at)}</span>
                  </div>
                  <div class="journal-session-content">
                    <div class="journal-session-title-row">
                      <strong>{session.project_name || t('journal.unassigned')}</strong>
                      {#if session.needs_review}
                        <span class="journal-review-badge">{t('journal.reviewBadge')}</span>
                      {/if}
                    </div>
                    <p>{session.task_summary || session.primary_app}</p>
                    <div class="journal-session-meta">
                      <span>{session.primary_app}</span>
                      <span>{formatDurationLocalized(session.duration, { compact: true })}</span>
                      <span>{t('journal.confidence', { confidence: session.confidence })}</span>
                    </div>
                    {#if session.evidence?.length}
                      <div class="journal-evidence" aria-label={t('journal.evidence')}>
                        {#each session.evidence.slice(0, 4) as evidence}
                          <span title={evidence}>{evidence}</span>
                        {/each}
                      </div>
                    {/if}
                    <div class="journal-review-controls">
                      <label>
                        <span>{t('journal.project')}</span>
                        <select
                          class="page-control-input"
                          value={draftProjectKeys[session.id] || ''}
                          disabled={reviewingSessionId === session.id}
                          on:change={(event) => updateDraftProject(session.id, event.currentTarget.value)}
                        >
                          <option value="">{t('journal.unassigned')}</option>
                          {#each projectRules as rule (rule.project_key)}
                            <option value={rule.project_key}>{rule.project_name}</option>
                          {/each}
                        </select>
                      </label>
                      <label class="journal-summary-field">
                        <span>{t('journal.taskSummary')}</span>
                        <input
                          class="page-control-input"
                          value={draftSummaries[session.id] || ''}
                          disabled={reviewingSessionId === session.id}
                          on:input={(event) => updateDraftSummary(session.id, event.currentTarget.value)}
                        />
                      </label>
                      <div class="journal-review-actions">
                        <button
                          type="button"
                          class="page-control-btn journal-confirm-btn"
                          disabled={reviewingSessionId === session.id || !draftProjectKeys[session.id]}
                          on:click={() => reviewSession(session, 'included')}
                        >
                          {reviewingSessionId === session.id ? t('journal.reviewing') : t('journal.confirmSession')}
                        </button>
                        <button
                          type="button"
                          class="page-control-btn"
                          disabled={reviewingSessionId === session.id}
                          on:click={() => reviewSession(session, 'private')}
                        >
                          {t('journal.markPrivate')}
                        </button>
                        <button
                          type="button"
                          class="page-control-btn journal-exclude-btn"
                          disabled={reviewingSessionId === session.id}
                          on:click={() => reviewSession(session, 'excluded')}
                        >
                          {t('journal.excludeSession')}
                        </button>
                      </div>
                    </div>
                  </div>
                </article>
              {/each}
            </div>
          {:else}
            <div class="journal-empty">{t('journal.empty')}</div>
          {/if}
        </section>
      </div>

      <section class="journal-section journal-preview-section">
        <div class="journal-section-head">
          <h3>{t('journal.exportPreview')}</h3>
        </div>
        {#if previewError}
          <p class="journal-preview-error" role="alert">{previewError}</p>
        {/if}
        <pre class="journal-export-preview" data-testid="journal-export-preview">{previewMarkdown || t('journal.noPreview')}</pre>
        <div class="journal-export-actions">
          {#if obsidianManualCopyEnabled}
            <button
              type="button"
              class="page-action-brand"
              disabled={!previewMarkdown}
              on:click={copyPreviewMarkdown}
            >
              {t('journal.copyPreview')}
            </button>
          {:else if obsidianWriteEnabled}
            <button
              type="button"
              class="page-action-brand"
              disabled={!previewMarkdown || !previewConfirmation || exporting}
              on:click={exportToObsidian}
            >
              {exporting ? t('journal.exporting') : t('journal.confirmExport')}
            </button>
          {/if}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .journal-page-shell {
    padding-top: 1.5rem;
  }

  .journal-title-badge {
    color: #0f766e;
    background: linear-gradient(145deg, rgba(240, 253, 250, 0.98), rgba(255, 255, 255, 0.94));
  }

  .journal-toolbar {
    justify-content: flex-end;
  }

  .journal-spin {
    animation: journal-spin 0.8s linear infinite;
  }

  .journal-error p,
  .journal-preview-error {
    margin: 0;
  }

  .journal-export-actions {
    padding: 0.75rem 1rem;
    display: flex;
    justify-content: flex-end;
    border-top: 1px solid rgba(148, 163, 184, 0.18);
  }

  .journal-loading {
    min-height: 12rem;
    display: grid;
    place-items: center;
  }

  .journal-loader {
    width: 1.5rem;
    height: 1.5rem;
    border: 2px solid rgba(99, 102, 241, 0.2);
    border-top-color: #6366f1;
    border-radius: 50%;
    animation: journal-spin 0.8s linear infinite;
  }

  .journal-metrics {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    border: 1px solid rgba(148, 163, 184, 0.2);
    background: var(--editorial-surface-featured);
    border-radius: 8px;
    overflow: hidden;
  }

  .journal-metric {
    min-height: 5.25rem;
    padding: 1rem 1.1rem;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 0.35rem;
    border-right: 1px solid rgba(148, 163, 184, 0.18);
  }

  .journal-metric:last-child {
    border-right: 0;
  }

  .journal-metric span,
  .journal-section-head span,
  .journal-project-name span,
  .journal-session-meta {
    color: #64748b;
    font-size: 0.75rem;
  }

  .journal-metric strong {
    color: #0f172a;
    font-size: 1.35rem;
    font-weight: 650;
  }

  .journal-metric-review strong {
    color: #b45309;
  }

  .journal-workspace {
    display: grid;
    grid-template-columns: minmax(0, 1.5fr) minmax(18rem, 0.8fr);
    gap: 1rem;
    align-items: start;
  }

  .journal-main-column {
    display: grid;
    gap: 1rem;
  }

  .journal-section {
    border: 1px solid rgba(148, 163, 184, 0.2);
    background: var(--editorial-surface-featured);
    border-radius: 8px;
    overflow: hidden;
  }

  .journal-section-head {
    min-height: 3rem;
    padding: 0.8rem 1rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid rgba(148, 163, 184, 0.18);
  }

  .journal-section-head h3 {
    margin: 0;
    color: #1e293b;
    font-size: 0.9rem;
    font-weight: 650;
  }

  .journal-project-list,
  .journal-session-list {
    display: grid;
  }

  .journal-project-row,
  .journal-session-row {
    display: flex;
    gap: 1rem;
    padding: 0.9rem 1rem;
    border-bottom: 1px solid rgba(148, 163, 184, 0.14);
  }

  .journal-project-row:last-child,
  .journal-session-row:last-child {
    border-bottom: 0;
  }

  .journal-project-row {
    align-items: center;
    justify-content: space-between;
  }

  .journal-project-name {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .journal-project-name div {
    min-width: 0;
    display: grid;
    gap: 0.2rem;
  }

  .journal-project-name strong,
  .journal-session-title-row strong {
    color: #1e293b;
    font-size: 0.82rem;
    font-weight: 650;
  }

  .journal-project-dot {
    width: 0.55rem;
    height: 0.55rem;
    flex: 0 0 auto;
    border-radius: 50%;
    background: #14b8a6;
  }

  .journal-project-dot-review {
    background: #f59e0b;
  }

  .journal-project-duration {
    color: #334155;
    font-size: 0.78rem;
    font-weight: 650;
    white-space: nowrap;
  }

  .journal-session-review {
    box-shadow: inset 3px 0 0 #f59e0b;
  }

  .journal-session-time {
    width: 3.25rem;
    flex: 0 0 auto;
    display: grid;
    align-content: start;
    gap: 0.15rem;
    font-variant-numeric: tabular-nums;
  }

  .journal-session-time strong {
    color: #0f172a;
    font-size: 0.78rem;
  }

  .journal-session-time span {
    color: #94a3b8;
    font-size: 0.72rem;
  }

  .journal-session-content {
    min-width: 0;
    flex: 1;
    display: grid;
    gap: 0.45rem;
  }

  .journal-session-title-row,
  .journal-session-meta,
  .journal-evidence {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .journal-session-content p {
    margin: 0;
    color: #475569;
    font-size: 0.8rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .journal-review-badge {
    padding: 0.15rem 0.4rem;
    border-radius: 999px;
    color: #92400e;
    background: #fef3c7;
    font-size: 0.65rem;
    font-weight: 650;
  }

  .journal-session-meta span:not(:last-child)::after {
    content: '·';
    margin-left: 0.4rem;
    color: #cbd5e1;
  }

  .journal-evidence span {
    max-width: 100%;
    padding: 0.22rem 0.45rem;
    border-radius: 4px;
    color: #475569;
    background: #f1f5f9;
    font-size: 0.66rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .journal-review-controls {
    margin-top: 0.2rem;
    padding-top: 0.75rem;
    display: grid;
    grid-template-columns: minmax(9rem, 0.7fr) minmax(12rem, 1.3fr);
    gap: 0.65rem;
    border-top: 1px dashed rgba(148, 163, 184, 0.24);
  }

  .journal-review-controls label {
    min-width: 0;
    display: grid;
    gap: 0.3rem;
  }

  .journal-review-controls label > span {
    color: #64748b;
    font-size: 0.68rem;
    font-weight: 600;
  }

  .journal-review-controls select,
  .journal-review-controls input {
    width: 100%;
    min-width: 0;
    font-size: 0.72rem;
  }

  .journal-review-actions {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .journal-review-actions button {
    min-height: 2rem;
    padding: 0.35rem 0.65rem;
    border-radius: 6px;
  }

  .journal-confirm-btn {
    color: #047857;
    border-color: rgba(16, 185, 129, 0.35);
    background: rgba(236, 253, 245, 0.82);
  }

  .journal-exclude-btn {
    color: #b91c1c;
  }

  .journal-preview-section {
    position: sticky;
    top: 1rem;
  }

  .journal-export-preview {
    min-height: 24rem;
    max-height: calc(100vh - 15rem);
    margin: 0;
    padding: 1rem;
    overflow: auto;
    color: #334155;
    background: #f8fafc;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.72rem;
    line-height: 1.65;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .journal-preview-error {
    padding: 0.75rem 1rem;
    color: #b91c1c;
    background: #fef2f2;
    font-size: 0.75rem;
  }

  .journal-empty {
    min-height: 6rem;
    display: grid;
    place-items: center;
    padding: 1rem;
    color: #94a3b8;
    font-size: 0.8rem;
  }

  :global(.dark) .journal-title-badge {
    color: #5eead4;
    background: linear-gradient(145deg, rgba(15, 118, 110, 0.24), rgba(22, 27, 34, 0.96));
  }

  :global(.dark) .journal-metric strong,
  :global(.dark) .journal-section-head h3,
  :global(.dark) .journal-project-name strong,
  :global(.dark) .journal-session-title-row strong,
  :global(.dark) .journal-session-time strong {
    color: #e6edf3;
  }

  :global(.dark) .journal-metric-review strong {
    color: #fbbf24;
  }

  :global(.dark) .journal-project-duration,
  :global(.dark) .journal-session-content p,
  :global(.dark) .journal-export-preview {
    color: #adbac7;
  }

  :global(.dark) .journal-evidence span,
  :global(.dark) .journal-export-preview {
    background: #0d1117;
  }

  :global(.dark) .journal-evidence span {
    color: #adbac7;
  }

  :global(.dark) .journal-review-badge {
    color: #fcd34d;
    background: rgba(146, 64, 14, 0.34);
  }

  :global(.dark) .journal-confirm-btn {
    color: #6ee7b7;
    border-color: rgba(16, 185, 129, 0.34);
    background: rgba(6, 78, 59, 0.28);
  }

  @media (max-width: 1020px) {
    .journal-metrics {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .journal-metric:nth-child(3) {
      border-right: 0;
    }

    .journal-metric:nth-child(n + 4) {
      border-top: 1px solid rgba(148, 163, 184, 0.18);
    }

    .journal-workspace {
      grid-template-columns: 1fr;
    }

    .journal-preview-section {
      position: static;
    }

    .journal-export-preview {
      max-height: 30rem;
    }
  }

  @media (max-width: 720px) {
    .journal-toolbar {
      width: 100%;
      justify-content: flex-start;
    }

    .journal-metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .journal-metric:nth-child(odd) {
      border-right: 1px solid rgba(148, 163, 184, 0.18);
    }

    .journal-metric:nth-child(even) {
      border-right: 0;
    }

    .journal-metric:nth-child(n + 3) {
      border-top: 1px solid rgba(148, 163, 184, 0.18);
    }

    .journal-session-row {
      gap: 0.65rem;
    }

    .journal-review-controls {
      grid-template-columns: 1fr;
    }

    .journal-summary-field,
    .journal-review-actions {
      grid-column: auto;
    }
  }

  @keyframes journal-spin {
    to { transform: rotate(360deg); }
  }
</style>

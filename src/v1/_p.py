import io

p = 'crates/moss/src/http.rs'
src = io.open(p, encoding='utf-8').read()

old = """    let runner = Arc::clone(&state.capture);
    let policy = policy.clone();
    let fqn_str = fqn.clone();
    let run_id = uuid::Uuid::now_v7().to_string();
    let mut announced = RunInfo {
        fqn: fqn_str.clone(),
        run_id: run_id.clone(),
        started_at: chrono::Utc::now(),
        phase: "imprint".into(),
        error: None,
        checkpoint: None,
        ferried_to: None,
    };
    state.capture.announce(announced.clone());
    let task_fqn = fqn_str.clone();
    let task_run = run_id.clone();
    tokio::spawn(async move {
        // The runner records progress under its own run id; the spawn carries
        // the caller-visible one.
        let _ = task_run;
        if let Err(e) = runner
            .execute_named(&task_fqn, &policy, &workload, &task_run)
            .await
        {
            tracing::warn!(offering = %task_fqn, error = %e, "capture run failed");
        }
    });
    announced.phase = "accepted".into();
    Ok(Json(
        serde_json::json!({ "data": { "run": announced } }),
    ))"""
new = """    let runner = Arc::clone(&state.capture);
    let policy = policy.clone();
    let fqn_str = fqn.clone();
    let run_id = uuid::Uuid::now_v7().to_string();
    let mut announced = RunInfo {
        fqn: fqn_str.clone(),
        run_id: run_id.clone(),
        started_at: chrono::Utc::now(),
        phase: "imprint".into(),
        error: None,
        checkpoint: None,
        ferried_to: None,
    };
    state.capture.announce(announced.clone());

    // Track the capture as a job (the data plane's async contract).
    let job_id = state.jobs.start("capture", &fqn_str);

    let task_fqn = fqn_str.clone();
    let task_run = run_id.clone();
    let jobs = state.jobs.clone();
    tokio::spawn(async move {
        let _ = task_run;
        match runner
            .execute_named(&task_fqn, &policy, &workload, &task_run)
            .await
        {
            Ok(checkpoint) => {
                jobs.complete(
                    &job_id,
                    serde_json::json!({
                        "checkpoint": checkpoint.display().to_string(),
                    }),
                );
            }
            Err(e) => {
                jobs.fail(&job_id, &e);
                tracing::warn!(offering = %task_fqn, error = %e, "capture run failed");
            }
        }
    });
    announced.phase = "accepted".into();
    Ok(Json(
        serde_json::json!({ "data": { "run": announced }, "job_id": job_id }),
    ))"""
assert src.count(old) == 1, "capture spawn"
src = src.replace(old, new)

io.open(p, 'w', encoding='utf-8', newline='').write(src)
print("capture->jobs wired")

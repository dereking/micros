// momo — a Maimemo-flavoured vocabulary trainer for Micro OS.
//
// The App follows the staged recall loop from the brief:
//   1. hear the word, guess the meaning;
//   2. reveal the spelling, guess again;
//   3. reveal an example sentence with the word blanked out;
//   4. reveal the definition and self-grade.
//
// Scheduling follows the Ebbinghaus ladder (5m, 30m, 12h, 1d, 2d, 4d, 7d,
// 15d, 30d). Every word's stage and last-review timestamp live in the App's
// namespaced KV store, so the plan survives relaunches. Up to five local users
// share one install; every storage key is written as `<ns><key>` where the
// default user keeps the legacy empty prefix and users 2..5 use `u2:`..`u5:`.
// `activeUser` / `userCount` are the only global config keys; `goal` and the
// Maimemo token are per-user config entries with the same prefix. The Sync tab
// imports the active account's unfinished today_items through the native host's
// header-aware HTTP call, reads the daily progress and per-word study records,
// then asks net.lookupWords for Chinese definitions/examples because the
// Maimemo OpenAPI does not expose built-in meanings. There are no arrays or
// user functions in this dialect: the active queue is ten KV-addressed slots.

app({ id: "momo", name: "momo", icon: "edit" });

// Session state.
const mode = state(0); // 0 idle, 1 studying, 2 round complete
const phase = state(0); // 0 listen, 1 spelling, 2 example, 3 definition
const idx = state(0); // current word 0..9
const curWord = state(""); // current spelling (for the speaker)
const scanFrom = state(0); // fairness cursor for the next-word scan
const roundDone = state(0);
const roundTotal = state(0);
const refresh = state(0); // bump after every KV write to re-run bindings
const resetArmed = state(0);
// Maimemo account sync: today items, daily progress and per-word records.
const syncStatus = state("");
const syncTotal = state(0);
const syncDone = state(0);
const syncWords = state("");
const syncImported = state(0);
const mmaFinished = state(0);
const mmaTotal = state(0);
const mmaMinutes = state(0);
const syncVocIds = state("");
const syncLookup = state("");
// After a successful 认识 rating the next word has already been chosen, but
// the current word's meaning stays on screen until the learner taps the blank
// study surface. `reviewIdx` points at the word being reported.
const waiting = state(0);
const reviewIdx = state(0);
// Multi-user state. Every study key is stored as `<ns><key>`; the first user
// keeps the legacy unprefixed keys so an existing install upgrades in place.
// `userCount` / `activeUser` live in the shared config document, while the
// per-user `goal` and `maimemoToken` config keys carry the same prefix.
const ns = state("");
const activeUser = state(1);
const userCount = state(1);
const booted = state(0);
const userStatus = state("");
const syncUser = state(0); // active user at the time the sync request started
const batchUser = state(0); // active user at the time a batch lookup started
// A sync chain is serialized: while one is in flight no second sync may start,
// because a new request would overwrite `syncUser` and let the old response
// pass the user guard. The chain clears this on completion or cancellation.
const syncBusy = state(0);
// Display name of the active user, plus the editable profile drafts shown on
// the 用户 tab. The drafts are loaded from the active user's config keys on
// boot and on every switch, so unsaved edits never leak between accounts.
const userName = state("");
const nameDraft = state("");
const tokenDraft = state("");
// `poolNext` lives in KV rather than a state so the cursor survives a
// relaunch. The pool itself is compact (word + voc_id only): the rest of
// today's list fits on ESP32 NVS while full dictionary payloads stay limited
// to the current ten-word batch.

ui.mount(
  ui.tabview([
    {
      title: "学习",
      content: ui.column([
        // Boot initializer. It is the first node (and therefore the first
        // binding the runtime evaluates), so every later binding already sees
        // the restored active user and its key prefix. Writing state from a
        // binding is safe here: the guard makes it a one-shot operation.
        ui.place(
          ui.text(
            bind(() => {
              if (booted.value === 0) {
                let count = config.getNumber("userCount");
                if (count < 1) {
                  count = 1;
                }
                if (count > 5) {
                  count = 5;
                }
                let active = config.getNumber("activeUser");
                if (active < 1) {
                  active = 1;
                }
                if (active > count) {
                  active = 1;
                }
                userCount.value = count;
                activeUser.value = active;
                if (active === 1) {
                  ns.value = "";
                } else {
                  ns.value = `u${active}:`;
                }
                let savedName = config.getText(`${ns.value}name`);
                if (savedName === "") {
                  if (active === 1) {
                    savedName = "默认用户";
                  } else {
                    savedName = `用户 ${active}`;
                  }
                }
                userName.value = savedName;
                nameDraft.value = savedName;
                tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                booted.value = 1;
              }
              return "";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 1, width: 1, anchor: { left: 0, top: 0 } },
        ),
        ui.place(ui.surface("canvas", {
          onClick: () => {
            if (waiting.value === 1) {
              waiting.value = 0;
              refresh.value++;
              if (mode.value === 1) {
                audio.speak(curWord.value);
              }
            }
          },
        }), {
          anchor: { left: 0, right: 0, top: 0, bottom: 0 },
        }),
        ui.place(ui.surface("paper", {
          onClick: () => {
            if (waiting.value === 1) {
              waiting.value = 0;
              refresh.value++;
              if (mode.value === 1) {
                audio.speak(curWord.value);
              }
            }
          },
        }), {
          height: 220,
          anchor: { left: 24, right: 24, top: 104 },
        }),
        ui.place(ui.surface("sage"), {
          height: 28,
          width: 220,
          anchor: { right: 24, top: 12 },
        }),
        ui.place(
          ui.text(
            bind(() => `MOMO · ${userName.value}`),
            {
              font: "uiSans",
              size: 12,
              weight: "regular",
              lineHeight: 14,
            },
          ),
          { height: 16, width: 210, anchor: { left: 24, top: 18 } },
        ),
        ui.place(ui.surface("terracotta"), {
          height: 32,
          width: 4,
          anchor: { left: 12, top: 38 },
        }),
        ui.place(
          ui.text("今天的单词", {
            font: "uiSans",
            size: 24,
            weight: "regular",
            lineHeight: 32,
          }),
          { height: 32, width: 300, anchor: { left: 24, top: 38 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const raw = config.getNumber(`${ns.value}goal`);
              let goal = 5;
              if (raw > 0) {
                goal = raw;
              }
              return `新词 ${kv.getNumber(`${ns.value}newToday`)} / ${goal}  ·  连续 ${kv.getNumber(`${ns.value}streak`)} 天`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 16, width: 196, anchor: { right: 36, top: 18 } },
        ),
        ui.place(
          ui.progress(
            bind(() => {
              if (roundTotal.value > 0) {
                let shown = roundDone.value;
                if (shown > roundTotal.value) {
                  shown = roundTotal.value;
                }
                return shown / roundTotal.value;
              }
              return 0;
            }),
          ),
          { height: 6, anchor: { left: 24, right: 24, top: 76 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (roundTotal.value > 0) {
                let shown = roundDone.value;
                if (shown > roundTotal.value) {
                  shown = roundTotal.value;
                }
                const pending = kv.getNumber(`${ns.value}poolCount`) - kv.getNumber(`${ns.value}poolNext`);
                if (pending > 0) {
                  return `本轮 ${shown} / ${roundTotal.value}  ·  待加载 ${pending}`;
                }
                return `本轮 ${shown} / ${roundTotal.value}`;
              }
              return "本轮 0 / 0";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 14, anchor: { left: 24, right: 24, top: 88 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (waiting.value === 1) {
                return "释义 · 学习反馈";
              }
              if (mode.value === 0) {
                return "准备开始";
              }
              if (mode.value === 2) {
                return "本轮完成";
              }
              if (phase.value === 0) {
                return "第 1 步 · 听音辨义";
              }
              if (phase.value === 1) {
                return "第 2 步 · 看词辨义";
              }
              if (phase.value === 2) {
                return "第 3 步 · 例句回忆";
              }
              return "释义 · 自我评估";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 16, anchor: { left: 44, right: 24, top: 116 } },
        ),

        ui.place(
          ui.text(
            bind(() => {
              if (waiting.value === 1) {
                return kv.getText(`${ns.value}rw`);
              }
              if (mode.value === 0) {
                return "momo";
              }
              if (mode.value === 2) {
                return "今日完成";
              }
              if (phase.value === 0) {
                return "听音辨义";
              }
              if (phase.value === 2) {
                return "例句回忆";
              }
              return curWord.value;
            }),
            { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          ),
          { height: 44, anchor: { left: 44, right: 180, top: 136 } },
        ),
        ui.place(
          ui.show(
            ui.button("播放发音", {
              onClick: () => {
                if (curWord.value !== "") {
                  audio.speak(curWord.value);
                }
              },
            }),
            bind(() => {
              if (mode.value !== 1) {
                return false;
              }
              return waiting.value === 0;
            }),
          ),
          { width: 120, height: 40, anchor: { right: 44, top: 138 } },
        ),

        ui.place(
          ui.text(
            bind(() => {
              if (waiting.value === 1) {
                if (mode.value === 2) {
                  return "点空白处查看今日总结";
                }
                return "点空白处继续下一个词";
              }
              if (mode.value === 0) {
                return "每天一点，记住更多";
              }
              if (mode.value === 2) {
                return "今天的词已经安排好了";
              }
              if (phase.value === 0) {
                return "点「播放发音」，听读音想词义";
              }
              if (phase.value === 2) {
                return "空格处应该填哪个词？";
              }
              const p = kv.getText(`${ns.value}p${idx.value}`);
              if (p !== "") {
                return `读音：${p}`;
              }
              return "读音：点播放";
            }),
            { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
          ),
          { height: 20, anchor: { left: 44, right: 44, top: 184 } },
        ),

        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 210 },
        }),
        ui.place(
          ui.text(
            bind(() => {
              if (waiting.value === 1) {
                const example = kv.getText(`${ns.value}re`);
                if (example !== "") {
                  return example;
                }
                return "（暂无例句）";
              }
              if (mode.value === 2) {
                return "";
              }
              if (phase.value === 2) {
                const cloze = kv.getText(`${ns.value}c${idx.value}`);
                if (cloze !== "") {
                  return cloze;
                }
                return "（例句加载中，可先点「不认识」看释义）";
              }
              if (phase.value === 3) {
                const example = kv.getText(`${ns.value}e${idx.value}`);
                if (example !== "") {
                  return example;
                }
                return "（暂无例句）";
              }
              return "";
            }),
            { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
          ),
          { height: 44, anchor: { left: 44, right: 44, top: 222 } },
        ),

        ui.place(
          ui.text(
            bind(() => {
              let at = idx.value;
              let reveal = false;
              if (waiting.value === 1) {
                at = reviewIdx.value;
                reveal = true;
              }
              if (phase.value === 3) {
                reveal = true;
              }
              if (reveal) {
                let definition = kv.getText(`${ns.value}d${at}`);
                let translation = kv.getText(`${ns.value}t${at}`);
                let seen = kv.getNumber(`${ns.value}m${at}`);
                if (waiting.value === 1) {
                  definition = kv.getText(`${ns.value}rd`);
                  translation = kv.getText(`${ns.value}rt`);
                  seen = kv.getNumber(`${ns.value}rm`);
                }
                if (translation !== "") {
                  if (definition === "") {
                    if (seen > 0) {
                      return `（词典暂无释义）\n${translation}  ·  墨墨 ${seen} 次`;
                    }
                    return `（词典暂无释义）\n${translation}`;
                  }
                  if (seen > 0) {
                    return `${definition}\n${translation}  ·  墨墨 ${seen} 次`;
                  }
                  return `${definition}\n${translation}`;
                }
                if (definition === "") {
                  if (seen > 0) {
                    return `（词典暂无释义）  ·  墨墨 ${seen} 次`;
                  }
                  return "（词典暂无释义）";
                }
                if (seen > 0) {
                  return `${definition}  ·  墨墨 ${seen} 次`;
                }
                return definition;
              }
              if (mode.value === 2) {
                return `今日已学 ${kv.getNumber(`${ns.value}newToday`)} 个新词`;
              }
              return "";
            }),
            { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 },
          ),
          { height: 50, anchor: { left: 44, right: 44, top: 272 } },
        ),

        ui.place(
          ui.row([
            ui.place(
              ui.show(
              ui.button("认识", {
                onClick: () => {
                  if (mode.value === 1) {
                    if (waiting.value === 0) {
                      const i = idx.value;
                    const last = kv.getNumber(`${ns.value}l${i}`);
                    const s = kv.getNumber(`${ns.value}s${i}`);
                    let countK = kv.getNumber(`${ns.value}rCount`);
                    if (countK < 1) {
                      countK = 10;
                    }
                    if (countK > 10) {
                      countK = 10;
                    }
                    const isNew = last === 0;
                    const now = clock.now();
                    let next = 0;
                    if (phase.value === 0) {
                      if (isNew) {
                        next = 3;
                      } else {
                        next = s + 2;
                        if (next > 8) {
                          next = 8;
                        }
                      }
                    } else {
                      if (isNew) {
                        next = 2;
                      } else {
                        next = s + 1;
                        if (next > 8) {
                          next = 8;
                        }
                      }
                    }
                    if (isNew) {
                      kv.setNumber(`${ns.value}newToday`, kv.getNumber(`${ns.value}newToday`) + 1);
                    }
                    kv.setNumber(`${ns.value}s${i}`, next);
                    kv.setNumber(`${ns.value}l${i}`, now);
                    roundDone.value++;
                    scanFrom.value = (i + 1) % countK;
                    waiting.value = 1;
                    reviewIdx.value = i;
                    // Keep the graded word's feedback independent of the slots.
                    // When this was the last word of a batch, the next batch is
                    // loaded immediately into w0..w9; without this snapshot the
                    // feedback would show the new batch's word instead.
                    kv.setText(`${ns.value}rw`, kv.getText(`${ns.value}w${i}`));
                    kv.setText(`${ns.value}re`, kv.getText(`${ns.value}e${i}`));
                    kv.setText(`${ns.value}rd`, kv.getText(`${ns.value}d${i}`));
                    kv.setText(`${ns.value}rt`, kv.getText(`${ns.value}t${i}`));
                    kv.setText(`${ns.value}rp`, kv.getText(`${ns.value}p${i}`));
                    kv.setNumber(`${ns.value}rm`, kv.getNumber(`${ns.value}m${i}`));
                    const nowS = clock.now();
                    const localS = nowS + 28800000;
                    const todayS = (localS - (localS % 86400000)) / 86400000;
                    const lastDay = kv.getNumber(`${ns.value}lastDay`);
                    let streakNow = kv.getNumber(`${ns.value}streak`);
                    if (lastDay !== todayS) {
                      if (lastDay === todayS - 1) {
                        streakNow = streakNow + 1;
                      } else {
                        streakNow = 1;
                      }
                      kv.setNumber(`${ns.value}streak`, streakNow);
                      kv.setNumber(`${ns.value}lastDay`, todayS);
                    }
                    const now2 = clock.now();
                    const local2 = now2 + 28800000;
                    const today2 = (local2 - (local2 % 86400000)) / 86400000;
                    if (kv.getNumber(`${ns.value}day`) !== today2) {
                      kv.setNumber(`${ns.value}day`, today2);
                      kv.setNumber(`${ns.value}newToday`, 0);
                    }
                    let nextIdx = 0 - 1;
                    let k = 0;
                    while (k < countK) {
                      let i2 = scanFrom.value + k;
                      if (i2 >= countK) {
                        i2 = i2 - countK;
                      }
                      if (nextIdx < 0) {
                        if (i2 !== i) {
                          const last2 = kv.getNumber(`${ns.value}l${i2}`);
                          if (last2 > 0) {
                            const s2 = kv.getNumber(`${ns.value}s${i2}`);
                            let gap2 = 5184000000;
                            if (s2 === 0) {
                              gap2 = 300000;
                            } else if (s2 === 1) {
                              gap2 = 1800000;
                            } else if (s2 === 2) {
                              gap2 = 43200000;
                            } else if (s2 === 3) {
                              gap2 = 86400000;
                            } else if (s2 === 4) {
                              gap2 = 172800000;
                            } else if (s2 === 5) {
                              gap2 = 345600000;
                            } else if (s2 === 6) {
                              gap2 = 604800000;
                            } else if (s2 === 7) {
                              gap2 = 1296000000;
                            } else if (s2 === 8) {
                              gap2 = 2592000000;
                            }
                            if (last2 + gap2 <= now2) {
                              nextIdx = i2;
                            }
                          }
                        }
                      }
                      k = k + 1;
                    }
                    if (nextIdx < 0) {
                      k = 0;
                      while (k < countK) {
                        let j2 = scanFrom.value + k;
                        if (j2 >= countK) {
                          j2 = j2 - countK;
                        }
                        if (nextIdx < 0) {
                          if (kv.getNumber(`${ns.value}l${j2}`) === 0) {
                            nextIdx = j2;
                          }
                        }
                        k = k + 1;
                      }
                    }
                    if (nextIdx >= 0) {
                      idx.value = nextIdx;
                      phase.value = 0;
                      mode.value = 1;
                      curWord.value = kv.getText(`${ns.value}w${nextIdx}`);
                    } else {
                      const poolCount = kv.getNumber(`${ns.value}poolCount`);
                      if (kv.getNumber(`${ns.value}poolNext`) < poolCount) {
                        // The current ten-word batch is done. Pull the next
                        // compact slice from the pool and keep the same ten
                        // slots, so storage stays bounded on ESP32.
                        const base = kv.getNumber(`${ns.value}poolNext`);
                        const pool = kv.getText(`${ns.value}pool`);
                        let batch = 0;
                        let firstNext = "";
                        let lookupNext = "";
                        let i4 = 0;
                        while (i4 < 10) {
                          const at = base + i4;
                          let spelling = "";
                          if (at < poolCount) {
                            spelling = json.getText(pool, `${at}`);
                          }
                          if (spelling !== "") {
                            kv.setText(`${ns.value}w${i4}`, spelling);
                            kv.remove(`${ns.value}v${i4}`);
                            kv.remove(`${ns.value}s${i4}`);
                            kv.remove(`${ns.value}l${i4}`);
                            kv.remove(`${ns.value}d${i4}`);
                            kv.remove(`${ns.value}e${i4}`);
                            kv.remove(`${ns.value}c${i4}`);
                            kv.remove(`${ns.value}t${i4}`);
                            kv.remove(`${ns.value}p${i4}`);
                            kv.remove(`${ns.value}m${i4}`);
                            kv.remove(`${ns.value}r${i4}`);
                            if (i4 === 0) {
                              firstNext = spelling;
                            }
                            lookupNext = `${lookupNext}${spelling}\n`;
                            batch = batch + 1;
                          } else {
                            kv.setText(`${ns.value}w${i4}`, "");
                            kv.remove(`${ns.value}v${i4}`);
                            kv.remove(`${ns.value}m${i4}`);
                            kv.remove(`${ns.value}r${i4}`);
                          }
                          i4 = i4 + 1;
                        }
                        if (batch > 0) {
                          kv.setNumber(`${ns.value}rCount`, batch);
                          kv.setNumber(`${ns.value}poolNext`, base + batch);
                          scanFrom.value = 0;
                          roundDone.value = 0;
                          roundTotal.value = batch;
                          idx.value = 0;
                          phase.value = 0;
                          mode.value = 1;
                          curWord.value = firstNext;
                          batchUser.value = activeUser.value;
                          net.lookupWords(lookupNext, (dictionary) => {
                            if (activeUser.value !== batchUser.value) {
                              return;
                            }
                            const entries = json.length(dictionary, "entries");
                            let d = 0;
                            while (d < entries) {
                              const word = json.getText(
                                dictionary,
                                `entries.${d}.word`,
                              );
                              let slot = 0 - 1;
                              let kk = 0;
                              while (kk < 10) {
                                if (slot < 0) {
                                  if (kv.getText(`${ns.value}w${kk}`) === word) {
                                    slot = kk;
                                  }
                                }
                                kk = kk + 1;
                              }
                              if (slot >= 0) {
                                const definition = json.getText(
                                  dictionary,
                                  `entries.${d}.definition`,
                                );
                                if (definition !== "") {
                                  kv.setText(`${ns.value}d${slot}`, definition);
                                  kv.setText(
                                    `${ns.value}p${slot}`,
                                    json.getText(
                                      dictionary,
                                      `entries.${d}.phonetic`,
                                    ),
                                  );
                                  kv.setText(
                                    `${ns.value}e${slot}`,
                                    json.getText(
                                      dictionary,
                                      `entries.${d}.example`,
                                    ),
                                  );
                                  kv.setText(
                                    `${ns.value}c${slot}`,
                                    json.getText(
                                      dictionary,
                                      `entries.${d}.cloze`,
                                    ),
                                  );
                                  kv.setText(
                                    `${ns.value}t${slot}`,
                                    json.getText(
                                      dictionary,
                                      `entries.${d}.translation`,
                                    ),
                                  );
                                }
                              }
                              d = d + 1;
                            }
                            refresh.value++;
                          });
                        } else {
                          audio.stop();
                          mode.value = 2;
                          phase.value = 4;
                        }
                      } else {
                        audio.stop();
                        mode.value = 2;
                        phase.value = 4;
                      }
                    }
                      refresh.value++;
                    }
                  }
                },
              }),
                bind(() => {
                  if (mode.value !== 1) {
                    return false;
                  }
                  return waiting.value === 0;
                }),
              ),
              { left: 0, width: 210, height: 44 },
            ),
            ui.place(
              ui.show(
              ui.button("不认识", {
                onClick: () => {
                  if (mode.value === 1) {
                    if (waiting.value === 0) {
                    if (phase.value < 3) {
                      phase.value++;
                    } else {
                      const i3 = idx.value;
                      let s3 = kv.getNumber(`${ns.value}s${i3}`);
                      s3 = s3 - 2;
                      if (s3 < 0) {
                        s3 = 0;
                      }
                      let gap3 = 300000;
                      if (s3 === 1) {
                        gap3 = 1800000;
                      } else if (s3 === 2) {
                        gap3 = 43200000;
                      } else if (s3 === 3) {
                        gap3 = 86400000;
                      } else if (s3 === 4) {
                        gap3 = 172800000;
                      } else if (s3 === 5) {
                        gap3 = 345600000;
                      } else if (s3 === 6) {
                        gap3 = 604800000;
                      } else if (s3 === 7) {
                        gap3 = 1296000000;
                      } else if (s3 === 8) {
                        gap3 = 2592000000;
                      }
                      kv.setNumber(`${ns.value}s${i3}`, s3);
                      kv.setNumber(`${ns.value}l${i3}`, clock.now() - gap3);
                      phase.value = 0;
                      mode.value = 1;
                      audio.speak(curWord.value);
                      refresh.value++;
                    }
                    }
                  }
                },
              }),
                bind(() => {
                  if (mode.value !== 1) {
                    return false;
                  }
                  return waiting.value === 0;
                }),
              ),
              { left: 220, width: 200, height: 44 },
            ),
          ]),
          { height: 44, anchor: { left: 24, right: 24, top: 332 } },
        ),

        ui.place(
          ui.show(
              ui.button("开始今日学习", {
                onClick: () => {
                  waiting.value = 0;
                  reviewIdx.value = 0;
                  // First run bootstrap. A successful Maimemo sync replaces
                  // these ten slots with today's real words; every binding and
                  // scheduler branch reads this one uniform KV source.
                  if (kv.getText(`${ns.value}w0`) === "") {
                    kv.setText(`${ns.value}w0`, "abandon");
                    kv.setText(`${ns.value}d0`, "v. 放弃；抛弃；遗弃");
                    kv.setText(`${ns.value}p0`, "əˈbændən");
                    kv.setText(`${ns.value}e0`, "They had to abandon the plan because of the storm.");
                    kv.setText(`${ns.value}c0`, "They had to ______ the plan because of the storm.");
                    kv.setText(`${ns.value}t0`, "由于暴风雨，他们不得不放弃这个计划。");
                    kv.setText(`${ns.value}w1`, "absorb");
                    kv.setText(`${ns.value}d1`, "v. 吸收；使全神贯注");
                    kv.setText(`${ns.value}p1`, "əbˈzɔːb");
                    kv.setText(`${ns.value}e1`, "Plants absorb water through their roots.");
                    kv.setText(`${ns.value}c1`, "Plants ______ water through their roots.");
                    kv.setText(`${ns.value}t1`, "植物通过根吸收水分。");
                    kv.setText(`${ns.value}w2`, "access");
                    kv.setText(`${ns.value}d2`, "v. 访问，存取；n. 通道，使用权");
                    kv.setText(`${ns.value}p2`, "ˈækses");
                    kv.setText(`${ns.value}e2`, "Students can access the library database from home.");
                    kv.setText(`${ns.value}c2`, "Students can ______ the library database from home.");
                    kv.setText(`${ns.value}t2`, "学生可以在家访问图书馆数据库。");
                    kv.setText(`${ns.value}w3`, "accurate");
                    kv.setText(`${ns.value}d3`, "adj. 准确的，精确的");
                    kv.setText(`${ns.value}p3`, "ˈækjərət");
                    kv.setText(`${ns.value}e3`, "The report gives an accurate picture of the market.");
                    kv.setText(`${ns.value}c3`, "The report gives an ______ picture of the market.");
                    kv.setText(`${ns.value}t3`, "这份报告准确描述了市场状况。");
                    kv.setText(`${ns.value}w4`, "achieve");
                    kv.setText(`${ns.value}d4`, "v. 实现，达到；完成");
                    kv.setText(`${ns.value}p4`, "əˈtʃiːv");
                    kv.setText(`${ns.value}e4`, "She worked hard to achieve her goal.");
                    kv.setText(`${ns.value}c4`, "She worked hard to ______ her goal.");
                    kv.setText(`${ns.value}t4`, "她努力工作以实现自己的目标。");
                    kv.setText(`${ns.value}w5`, "adapt");
                    kv.setText(`${ns.value}d5`, "v. 适应；改编");
                    kv.setText(`${ns.value}p5`, "əˈdæpt");
                    kv.setText(`${ns.value}e5`, "It takes time to adapt to a new environment.");
                    kv.setText(`${ns.value}c5`, "It takes time to ______ to a new environment.");
                    kv.setText(`${ns.value}t5`, "适应新环境需要时间。");
                    kv.setText(`${ns.value}w6`, "adopt");
                    kv.setText(`${ns.value}d6`, "v. 采用，采纳；收养");
                    kv.setText(`${ns.value}p6`, "əˈdɒpt");
                    kv.setText(`${ns.value}e6`, "The company decided to adopt a new strategy.");
                    kv.setText(`${ns.value}c6`, "The company decided to ______ a new strategy.");
                    kv.setText(`${ns.value}t6`, "公司决定采用一项新策略。");
                    kv.setText(`${ns.value}w7`, "advantage");
                    kv.setText(`${ns.value}d7`, "n. 优势，有利条件");
                    kv.setText(`${ns.value}p7`, "ədˈvɑːntɪdʒ");
                    kv.setText(`${ns.value}e7`, "Speaking two languages is a big advantage.");
                    kv.setText(`${ns.value}c7`, "Speaking two languages is a big ______.");
                    kv.setText(`${ns.value}t7`, "会说两种语言是很大的优势。");
                    kv.setText(`${ns.value}w8`, "affect");
                    kv.setText(`${ns.value}d8`, "v. 影响；感动");
                    kv.setText(`${ns.value}p8`, "əˈfekt");
                    kv.setText(`${ns.value}e8`, "The weather can affect your mood.");
                    kv.setText(`${ns.value}c8`, "The weather can ______ your mood.");
                    kv.setText(`${ns.value}t8`, "天气会影响你的心情。");
                    kv.setText(`${ns.value}w9`, "afford");
                    kv.setText(`${ns.value}d9`, "v. 负担得起；提供");
                    kv.setText(`${ns.value}p9`, "əˈfɔːd");
                    kv.setText(`${ns.value}e9`, "We cannot afford to waste any more time.");
                    kv.setText(`${ns.value}c9`, "We cannot ______ to waste any more time.");
                    kv.setText(`${ns.value}t9`, "我们不能再浪费时间了。");
                    kv.setNumber(`${ns.value}rCount`, 10);
                  }

                  // The plan is read from KV on demand, so there is no hydration pass: bindings
                  // sample the store, and handlers write it. The daily goal is also persisted.
                  const now0 = clock.now();
                  const local0 = now0 + 28800000;
                  const today0 = (local0 - (local0 % 86400000)) / 86400000;
                  if (kv.getNumber(`${ns.value}day`) !== today0) {
                    kv.setNumber(`${ns.value}day`, today0);
                    kv.setNumber(`${ns.value}newToday`, 0);
                  }
                  let count0 = kv.getNumber(`${ns.value}rCount`);
                  if (count0 < 1) {
                    count0 = 10;
                  }
                  if (count0 > 10) {
                    count0 = 10;
                  }
                  scanFrom.value = 0;
                  roundDone.value = 0;
                  let dueCount = 0;
                  let unseen = 0;
                  let i0 = 0;
                  while (i0 < count0) {
                    const last0 = kv.getNumber(`${ns.value}l${i0}`);
                    if (last0 > 0) {
                      const s0 = kv.getNumber(`${ns.value}s${i0}`);
                      let gap0 = 5184000000;
                      if (s0 === 0) {
                        gap0 = 300000;
                      } else if (s0 === 1) {
                        gap0 = 1800000;
                      } else if (s0 === 2) {
                        gap0 = 43200000;
                      } else if (s0 === 3) {
                        gap0 = 86400000;
                      } else if (s0 === 4) {
                        gap0 = 172800000;
                      } else if (s0 === 5) {
                        gap0 = 345600000;
                      } else if (s0 === 6) {
                        gap0 = 604800000;
                      } else if (s0 === 7) {
                        gap0 = 1296000000;
                      } else if (s0 === 8) {
                        gap0 = 2592000000;
                      }
                      if (last0 + gap0 <= now0) {
                        dueCount = dueCount + 1;
                      }
                    } else {
                      unseen = unseen + 1;
                    }
                    i0 = i0 + 1;
                  }
                  // The synced pool is today's actual plan. The settings goal is
                  // a target, not a hard stop: let a started round cover every
                  // due/unseen word in the active batch so the next batch can
                  // load automatically instead of ending at the goal.
                  roundTotal.value = dueCount + unseen;
                  let first = 0 - 1;
                  i0 = 0;
                  while (i0 < count0) {
                    if (first < 0) {
                      const last1 = kv.getNumber(`${ns.value}l${i0}`);
                      if (last1 > 0) {
                        const s1 = kv.getNumber(`${ns.value}s${i0}`);
                        let gap1 = 5184000000;
                        if (s1 === 0) {
                          gap1 = 300000;
                        } else if (s1 === 1) {
                          gap1 = 1800000;
                        } else if (s1 === 2) {
                          gap1 = 43200000;
                        } else if (s1 === 3) {
                          gap1 = 86400000;
                        } else if (s1 === 4) {
                          gap1 = 172800000;
                        } else if (s1 === 5) {
                          gap1 = 345600000;
                        } else if (s1 === 6) {
                          gap1 = 604800000;
                        } else if (s1 === 7) {
                          gap1 = 1296000000;
                        } else if (s1 === 8) {
                          gap1 = 2592000000;
                        }
                        if (last1 + gap1 <= now0) {
                          first = i0;
                        }
                      }
                    }
                    i0 = i0 + 1;
                  }
                  if (first < 0) {
                    i0 = 0;
                    while (i0 < count0) {
                      if (first < 0) {
                        if (kv.getNumber(`${ns.value}l${i0}`) === 0) {
                          first = i0;
                        }
                      }
                      i0 = i0 + 1;
                    }
                  }
                  if (first >= 0) {
                    idx.value = first;
                    curWord.value = kv.getText(`${ns.value}w${first}`);
                    phase.value = 0;
                    mode.value = 1;
                    audio.speak(curWord.value);
                  } else {
                    audio.stop();
                    phase.value = 4;
                    mode.value = 2;
                  }
                  refresh.value++;
                },
              }),
            bind(() => {
              if (mode.value === 1) {
                return false;
              }
              return waiting.value === 0;
            }),
          ),
          { height: 44, anchor: { left: 24, right: 24, top: 332 } },
        ),
      ]),
    },
    {
      title: "进度",
      content: ui.column([
        ui.place(ui.surface("canvas"), {
          anchor: { left: 0, right: 0, top: 0, bottom: 0 },
        }),
        ui.place(ui.surface("paper"), {
          height: 280,
          anchor: { left: 24, right: 24, top: 90 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 125 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 151 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 177 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 203 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 229 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 255 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 281 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 307 },
        }),
        ui.place(ui.surface("line"), {
          height: 1,
          anchor: { left: 44, right: 44, top: 333 },
        }),
        ui.place(
          ui.text("MEMORY · 记忆档案", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, width: 300, anchor: { left: 24, top: 16 } },
        ),
        ui.place(ui.surface("terracotta"), {
          height: 32,
          width: 4,
          anchor: { left: 12, top: 34 },
        }),
        ui.place(
          ui.text("下一次复习", {
            font: "uiSans",
            size: 24,
            weight: "regular",
            lineHeight: 32,
          }),
          { height: 32, width: 300, anchor: { left: 24, top: 34 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const total = kv.getNumber(`${ns.value}poolCount`);
              if (total > 0) {
                return `本组 ${kv.getNumber(`${ns.value}rCount`)} 个 · 今日队列 ${total} 个`;
              }
              return "按遗忘曲线安排，到期后自动回到今日队列。";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 16, anchor: { left: 24, right: 24, top: 68 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l0`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w0`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s0`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w0`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w0`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w0`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w0`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 100 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l1`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w1`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s1`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w1`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w1`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w1`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w1`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 126 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l2`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w2`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s2`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w2`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w2`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w2`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w2`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 152 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l3`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w3`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s3`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w3`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w3`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w3`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w3`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 178 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l4`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w4`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s4`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w4`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w4`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w4`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w4`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 204 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l5`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w5`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s5`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w5`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w5`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w5`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w5`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 230 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l6`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w6`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s6`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w6`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w6`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w6`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w6`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 256 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l7`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w7`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s7`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w7`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w7`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w7`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w7`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 282 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l8`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w8`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s8`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w8`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w8`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w8`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w8`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 308 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const last = kv.getNumber(`${ns.value}l9`);
              if (last === 0) {
                return `${kv.getText(`${ns.value}w9`)}  ·  新词`;
              }
              const s = kv.getNumber(`${ns.value}s9`);
              let gap = 5184000000;
              if (s === 0) {
                gap = 300000;
              } else if (s === 1) {
                gap = 1800000;
              } else if (s === 2) {
                gap = 43200000;
              } else if (s === 3) {
                gap = 86400000;
              } else if (s === 4) {
                gap = 172800000;
              } else if (s === 5) {
                gap = 345600000;
              } else if (s === 6) {
                gap = 604800000;
              } else if (s === 7) {
                gap = 1296000000;
              } else if (s === 8) {
                gap = 2592000000;
              }
              const due = last + gap - clock.now();
              if (due <= 0) {
                return `${kv.getText(`${ns.value}w9`)}  ·  第 ${s + 1} 阶  ·  待复习`;
              }
              const mins = (due - (due % 60000)) / 60000;
              if (mins < 60) {
                return `${kv.getText(`${ns.value}w9`)}  ·  第 ${s + 1} 阶  ·  ${mins} 分钟后`;
              }
              const hours = (mins - (mins % 60)) / 60;
              if (hours < 24) {
                return `${kv.getText(`${ns.value}w9`)}  ·  第 ${s + 1} 阶  ·  ${hours} 小时后`;
              }
              const days = (hours - (hours % 24)) / 24;
              return `${kv.getText(`${ns.value}w9`)}  ·  第 ${s + 1} 阶  ·  ${days} 天后`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 24, anchor: { left: 44, right: 44, top: 334 } },
        ),
      ]),
    },
    {
      title: "设置",
      content: ui.column([
        ui.place(ui.surface("canvas"), {
          anchor: { left: 0, right: 0, top: 0, bottom: 0 },
        }),
        ui.place(ui.surface("paper"), {
          height: 112,
          anchor: { left: 24, right: 24, top: 90 },
        }),
        ui.place(ui.surface("paper"), {
          height: 160,
          anchor: { left: 24, right: 24, top: 210 },
        }),
        ui.place(
          ui.text("PREFERENCES · 学习节奏", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, width: 320, anchor: { left: 24, top: 16 } },
        ),
        ui.place(ui.surface("terracotta"), {
          height: 32,
          width: 4,
          anchor: { left: 12, top: 34 },
        }),
        ui.place(
          ui.text("设置", {
            font: "uiSans",
            size: 24,
            weight: "regular",
            lineHeight: 32,
          }),
          { height: 32, width: 240, anchor: { left: 24, top: 34 } },
        ),
        ui.place(
          ui.text("每天的新词目标", {
            font: "uiSans",
            size: 18,
            weight: "regular",
            lineHeight: 24,
          }),
          { height: 24, width: 300, anchor: { left: 44, top: 108 } },
        ),
        ui.place(
          ui.row([
            ui.place(
              ui.button("3 个", {
                onClick: () => {
                  config.setNumber(`${ns.value}goal`, 3);
                  refresh.value++;
                },
              }),
              { left: 0, width: 90, height: 44 },
            ),
            ui.place(
              ui.button("5 个", {
                onClick: () => {
                  config.setNumber(`${ns.value}goal`, 5);
                  refresh.value++;
                },
              }),
              { left: 102, width: 90, height: 44 },
            ),
            ui.place(
              ui.button("8 个", {
                onClick: () => {
                  config.setNumber(`${ns.value}goal`, 8);
                  refresh.value++;
                },
              }),
              { left: 204, width: 90, height: 44 },
            ),
          ]),
          { height: 44, anchor: { left: 44, right: 44, top: 136 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const raw = config.getNumber(`${ns.value}goal`);
              let goal = 5;
              if (raw > 0) {
                goal = raw;
              }
              return `目标：${goal} 个新词 / 天（可超额）`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 16, anchor: { left: 44, right: 24, top: 184 } },
        ),
        ui.place(
          ui.text("学习数据", {
            font: "uiSans",
            size: 18,
            weight: "regular",
            lineHeight: 24,
          }),
          { height: 24, width: 240, anchor: { left: 44, top: 228 } },
        ),
        ui.place(
          ui.button("重置学习进度", {
            onClick: () => {
              if (resetArmed.value === 1) {
                let i = 0;
                while (i < 10) {
                  kv.remove(`${ns.value}s${i}`);
                  kv.remove(`${ns.value}l${i}`);
                  kv.remove(`${ns.value}m${i}`);
                  kv.remove(`${ns.value}r${i}`);
                  i = i + 1;
                }
                kv.remove(`${ns.value}day`);
                kv.remove(`${ns.value}newToday`);
                kv.remove(`${ns.value}lastDay`);
                kv.remove(`${ns.value}streak`);
                kv.setText(`${ns.value}pool`, "[]");
                kv.setNumber(`${ns.value}poolCount`, 0);
                kv.setNumber(`${ns.value}poolNext`, 0);
                kv.remove(`${ns.value}rw`);
                kv.remove(`${ns.value}re`);
                kv.remove(`${ns.value}rd`);
                kv.remove(`${ns.value}rt`);
                kv.remove(`${ns.value}rp`);
                kv.remove(`${ns.value}rm`);
                audio.stop();
                mode.value = 0;
                phase.value = 0;
                idx.value = 0;
                curWord.value = "";
                waiting.value = 0;
                reviewIdx.value = 0;
                roundDone.value = 0;
                roundTotal.value = 0;
                resetArmed.value = 0;
                syncUser.value = 0;
                batchUser.value = 0;
                syncStatus.value = "";
                refresh.value++;
              } else {
                resetArmed.value = 1;
              }
            },
          }),
          { left: 44, width: 190, height: 44, top: 254 },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (resetArmed.value === 1) {
                return "再点一次确认，数据不可恢复。";
              }
              return "只清空当前用户的学习记录，不影响其他用户与 App。";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 306 } },
        ),
        ui.place(
          ui.text("momo · Micro App  ·  今日词表 · 艾宾浩斯复习  ·  v1.3", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, anchor: { left: 24, right: 24, top: 392 } },
        ),
      ]),
    },
    {
      title: "同步",
      content: ui.column([
        ui.place(ui.surface("canvas"), {
          anchor: { left: 0, right: 0, top: 0, bottom: 0 },
        }),
        ui.place(ui.surface("paper"), {
          height: 88,
          anchor: { left: 24, right: 24, top: 92 },
        }),
        ui.place(ui.surface("paper"), {
          height: 96,
          anchor: { left: 24, right: 24, top: 270 },
        }),
        ui.place(
          ui.text("ACCOUNT · 墨墨同步", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, width: 320, anchor: { left: 24, top: 16 } },
        ),
        ui.place(ui.surface("terracotta"), {
          height: 32,
          width: 4,
          anchor: { left: 12, top: 34 },
        }),
        ui.place(
          ui.text("同步今日计划", {
            font: "uiSans",
            size: 24,
            weight: "regular",
            lineHeight: 32,
          }),
          { height: 32, width: 320, anchor: { left: 24, top: 34 } },
        ),
        ui.place(
          ui.text(
            "Token 按用户隔离，请到用户页与用户名一起管理；同步只读取今日词、进度和学习记录，不会提交复习结果。",
            {
              font: "uiSans",
              size: 12,
              weight: "regular",
              lineHeight: 14,
            },
          ),
          { height: 16, anchor: { left: 24, right: 24, top: 68 } },
        ),
        ui.place(
          ui.text("开放 API Token", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, width: 300, anchor: { left: 44, top: 104 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (refresh.value < 0) {
                return "";
              }
              const token = config.getText(`${ns.value}maimemoToken`);
              if (token === "") {
                return "未配置 · 请到用户页填写当前用户的 API Key";
              }
              return "已配置 · 可在用户页修改当前用户的 API Key";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 34, anchor: { left: 44, right: 44, top: 128 } },
        ),
        ui.place(
          ui.button("同步墨墨", {
            onClick: () => {
              if (syncBusy.value === 1) {
                syncStatus.value = "同步进行中，请稍候…";
                refresh.value++;
                return;
              }
              const token = config.getText(`${ns.value}maimemoToken`);
              if (token === "") {
                syncStatus.value = "请先在用户页填写当前用户的 API Key。";
              } else {
                syncBusy.value = 1;
                syncStatus.value = "正在同步…";
                syncUser.value = activeUser.value;
                net.httpRequestWithHeaders(
                  "POST",
                  "https://open.maimemo.com/open/api/v1/memo/study/get_today_items",
                  `Authorization: Bearer ${token}
Accept: application/json
Content-Type: application/json`,
                  "{}",
                  (response) => {
                    if (activeUser.value !== syncUser.value) {
                      syncBusy.value = 0;
                      return;
                    }
                    const total = json.length(response, "data.today_items");
                    syncTotal.value = total;
                    let done = 0;
                    let i = 0;
                    while (i < total) {
                      if (
                        json.getText(
                          response,
                          `data.today_items.${i}.is_finished`,
                        ) === "true"
                      ) {
                        done = done + 1;
                      }
                      i = i + 1;
                    }
                    syncDone.value = done;
                    let picked = 0;
                    let pool = "";
                    let poolCount = 0;
                    let preview = "";
                    let lookup = "";
                    let vocIds = "";
                    i = 0;
                    while (i < total) {
                      const spelling = json.getText(
                        response,
                        `data.today_items.${i}.voc_spelling`,
                      );
                      const finished = json.getText(
                        response,
                        `data.today_items.${i}.is_finished`,
                      );
                      const vocId = json.getText(
                        response,
                        `data.today_items.${i}.voc_id`,
                      );
                      let usable = spelling !== "";
                      if (finished === "true") {
                        usable = false;
                      }
                      if (poolCount > 63) {
                        usable = false;
                      }
                      if (usable) {
                        if (poolCount > 0) {
                          pool = `${pool},`;
                        }
                        pool = `${pool}"${spelling}"`;
                        poolCount = poolCount + 1;
                        if (poolCount === 1) {
                          preview = spelling;
                        } else if (poolCount === 2) {
                          preview = `${preview} · ${spelling}`;
                        } else if (poolCount === 3) {
                          preview = `${preview} · ${spelling}`;
                        }
                        if (picked < 10) {
                          const previous = kv.getText(`${ns.value}w${picked}`);
                          kv.setText(`${ns.value}w${picked}`, spelling);
                          kv.setText(`${ns.value}v${picked}`, vocId);
                          if (previous !== spelling) {
                            kv.remove(`${ns.value}s${picked}`);
                            kv.remove(`${ns.value}l${picked}`);
                            kv.remove(`${ns.value}d${picked}`);
                            kv.remove(`${ns.value}e${picked}`);
                            kv.remove(`${ns.value}c${picked}`);
                            kv.remove(`${ns.value}t${picked}`);
                            kv.remove(`${ns.value}p${picked}`);
                            kv.remove(`${ns.value}m${picked}`);
                            kv.remove(`${ns.value}r${picked}`);
                          }
                          lookup = `${lookup}${spelling}\n`;
                          if (vocId !== "") {
                            if (vocIds !== "") {
                              vocIds = `${vocIds},`;
                            }
                            vocIds = `${vocIds}"${vocId}"`;
                          }
                          picked = picked + 1;
                        }
                      }
                      i = i + 1;
                    }
                    syncWords.value = preview;
                    syncImported.value = poolCount;
                    syncLookup.value = lookup;
                    syncVocIds.value = vocIds;
                    if (poolCount > 0) {
                      kv.setText(`${ns.value}pool`, `[${pool}]`);
                      kv.setNumber(`${ns.value}poolCount`, poolCount);
                      kv.setNumber(`${ns.value}poolNext`, picked);
                      kv.setNumber(`${ns.value}rCount`, picked);
                      let clear = picked;
                      while (clear < 10) {
                        kv.setText(`${ns.value}w${clear}`, "");
                        kv.remove(`${ns.value}v${clear}`);
                        kv.remove(`${ns.value}m${clear}`);
                        kv.remove(`${ns.value}r${clear}`);
                        clear = clear + 1;
                      }
                    } else {
                      kv.setText(`${ns.value}pool`, "[]");
                      kv.setNumber(`${ns.value}poolCount`, 0);
                      kv.setNumber(`${ns.value}poolNext`, 0);
                    }
                    if (picked === 0) {
                      syncBusy.value = 0;
                      const message = json.getText(response, "errors.0.msg");
                      if (message !== "") {
                        syncStatus.value = `同步失败：${message}`;
                      } else if (json.getText(response, "success") === "true") {
                        syncStatus.value = "今日没有未完成单词。";
                      } else {
                        syncStatus.value = "同步失败：没有可读的今日词表。";
                      }
                    } else {
                      mode.value = 0;
                      phase.value = 0;
                      idx.value = 0;
                      curWord.value = "";
                      waiting.value = 0;
                      reviewIdx.value = 0;
                      roundDone.value = 0;
                      roundTotal.value = 0;
                      syncStatus.value = `已导入 ${syncImported.value} 个今日词，正在同步学习进度…`;
                      refresh.value++;
                      net.httpRequestWithHeaders(
                        "POST",
                        "https://open.maimemo.com/open/api/v1/memo/study/get_study_progress",
                        `Authorization: Bearer ${config.getText(`${ns.value}maimemoToken`)}
Accept: application/json
Content-Type: application/json`,
                        "{}",
                        (progress) => {
                          if (activeUser.value !== syncUser.value) {
                            syncBusy.value = 0;
                            return;
                          }
                          const finished = json.getNumber(
                            progress,
                            "data.progress.finished",
                          );
                          const planned = json.getNumber(
                            progress,
                            "data.progress.total",
                          );
                          const studyMs = json.getNumber(
                            progress,
                            "data.progress.study_time",
                          );
                          let minutes = 0;
                          if (studyMs > 0) {
                            minutes = (studyMs - (studyMs % 60000)) / 60000;
                          }
                          mmaFinished.value = finished;
                          mmaTotal.value = planned;
                          mmaMinutes.value = minutes;
                          syncStatus.value = "正在同步学习记录…";
                          refresh.value++;
                          net.httpRequestWithHeaders(
                            "POST",
                            "https://open.maimemo.com/open/api/v1/memo/study/query_study_records",
                            `Authorization: Bearer ${config.getText(`${ns.value}maimemoToken`)}
Accept: application/json
Content-Type: application/json`,
                            `{"voc_ids":[${syncVocIds.value}],"limit":10}`,
                            (records) => {
                              if (activeUser.value !== syncUser.value) {
                                syncBusy.value = 0;
                                return;
                              }
                              const count = json.length(
                                records,
                                "data.records",
                              );
                              let j = 0;
                              while (j < count) {
                                const vocId = json.getText(
                                  records,
                                  `data.records.${j}.voc_id`,
                                );
                                let slot = 0 - 1;
                                let k = 0;
                                while (k < 10) {
                                  if (slot < 0) {
                                    if (kv.getText(`${ns.value}v${k}`) === vocId) {
                                      slot = k;
                                    }
                                  }
                                  k = k + 1;
                                }
                                if (slot >= 0) {
                                  const studyCount = json.getNumber(
                                    records,
                                    `data.records.${j}.study_count`,
                                  );
                                  const feedback = json.getText(
                                    records,
                                    `data.records.${j}.last_response`,
                                  );
                                  const tag = json.getText(
                                    records,
                                    `data.records.${j}.tags.0`,
                                  );
                                  kv.setNumber(`${ns.value}m${slot}`, studyCount);
                                  kv.setText(`${ns.value}r${slot}`, feedback);
                                  let hasHistory = 0;
                                  if (studyCount > 0) {
                                    hasHistory = 1;
                                  }
                                  if (feedback === "WELL_FAMILIAR") {
                                    hasHistory = 1;
                                  }
                                  if (tag === "WELL_FAMILIAR") {
                                    hasHistory = 1;
                                  }
                                  if (hasHistory === 1) {
                                    if (kv.getNumber(`${ns.value}l${slot}`) === 0) {
                                      let stage = 0;
                                      if (studyCount >= 1) {
                                        stage = 3;
                                      }
                                      if (studyCount >= 2) {
                                        stage = 4;
                                      }
                                      if (studyCount >= 3) {
                                        stage = 5;
                                      }
                                      if (studyCount >= 4) {
                                        stage = 6;
                                      }
                                      if (studyCount >= 5) {
                                        stage = 7;
                                      }
                                      if (studyCount >= 8) {
                                        stage = 8;
                                      }
                                      if (feedback === "WELL_FAMILIAR") {
                                        stage = 8;
                                      }
                                      if (tag === "WELL_FAMILIAR") {
                                        stage = 8;
                                      }
                                      if (feedback === "FORGET") {
                                        if (stage > 2) {
                                          stage = stage - 2;
                                        }
                                      }
                                      if (feedback === "VAGUE") {
                                        if (stage > 1) {
                                          stage = stage - 1;
                                        }
                                      }
                                      let gap = 300000;
                                      if (stage === 1) {
                                        gap = 1800000;
                                      } else if (stage === 2) {
                                        gap = 43200000;
                                      } else if (stage === 3) {
                                        gap = 86400000;
                                      } else if (stage === 4) {
                                        gap = 172800000;
                                      } else if (stage === 5) {
                                        gap = 345600000;
                                      } else if (stage === 6) {
                                        gap = 604800000;
                                      } else if (stage === 7) {
                                        gap = 1296000000;
                                      } else if (stage === 8) {
                                        gap = 2592000000;
                                      }
                                      kv.setNumber(`${ns.value}s${slot}`, stage);
                                      kv.setNumber(
                                        `${ns.value}l${slot}`,
                                        clock.now() - gap,
                                      );
                                    }
                                  }
                                }
                                j = j + 1;
                              }
                              syncStatus.value = "正在获取释义…";
                              refresh.value++;
                              net.lookupWords(syncLookup.value, (dictionary) => {
                                if (activeUser.value !== syncUser.value) {
                                  syncBusy.value = 0;
                                  return;
                                }
                                const entries = json.length(dictionary, "entries");
                                let good = 0;
                                let d = 0;
                                while (d < entries) {
                                  const word = json.getText(
                                    dictionary,
                                    `entries.${d}.word`,
                                  );
                                  let slot = 0 - 1;
                                  let k = 0;
                                  while (k < 10) {
                                    if (slot < 0) {
                                      if (kv.getText(`${ns.value}w${k}`) === word) {
                                        slot = k;
                                      }
                                    }
                                    k = k + 1;
                                  }
                                  if (slot >= 0) {
                                    const definition = json.getText(
                                      dictionary,
                                      `entries.${d}.definition`,
                                    );
                                    if (definition !== "") {
                                      kv.setText(`${ns.value}d${slot}`, definition);
                                      kv.setText(
                                        `${ns.value}p${slot}`,
                                        json.getText(
                                          dictionary,
                                          `entries.${d}.phonetic`,
                                        ),
                                      );
                                      kv.setText(
                                        `${ns.value}e${slot}`,
                                        json.getText(
                                          dictionary,
                                          `entries.${d}.example`,
                                        ),
                                      );
                                      kv.setText(
                                        `${ns.value}c${slot}`,
                                        json.getText(
                                          dictionary,
                                          `entries.${d}.cloze`,
                                        ),
                                      );
                                      kv.setText(
                                        `${ns.value}t${slot}`,
                                        json.getText(
                                          dictionary,
                                          `entries.${d}.translation`,
                                        ),
                                      );
                                      good = good + 1;
                                    }
                                  }
                                  d = d + 1;
                                }
                                syncBusy.value = 0;
                                if (good > 0) {
                                  syncStatus.value = `已导入 ${syncImported.value} 个今日词，可开始学习。`;
                                } else {
                                  syncStatus.value = "词表已导入，但释义暂不可用。";
                                }
                                refresh.value++;
                              });
                            },
                          );
                        },
                      );
                    }
                  },
                );
              }
            },
          }),
          { left: 44, width: 170, height: 40, top: 188 },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (syncStatus.value === "") {
                return "尚未同步。";
              }
              return syncStatus.value;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 30, anchor: { left: 230, right: 44, top: 188 } },
        ),
        ui.place(
          ui.text(
            bind(
              () =>
                `墨墨今日：完成 ${syncDone.value} / ${syncTotal.value}  ·  已导入 ${syncImported.value}`,
            ),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 18, anchor: { left: 44, right: 44, top: 282 } },
        ),
        ui.place(
          ui.text(
            bind(
              () =>
                `学习进度：完成 ${mmaFinished.value} / ${mmaTotal.value}  ·  今日 ${mmaMinutes.value} 分钟`,
            ),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 18, anchor: { left: 44, right: 44, top: 304 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (syncWords.value === "") {
                return "今日词表：—";
              }
              return `今日词表：${syncWords.value}`;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 28, anchor: { left: 44, right: 44, top: 326 } },
        ),
        ui.place(
          ui.text(
            "Token 以明文保存在 OS config 配置文件中（macOS ~/.micros/momo/config.json、Web localStorage、ESP32 NVS）；离开设备后请在墨墨 App 内重置。macOS 与 ESP32 使用宿主内置的 HTTPS 客户端（不调用外部命令）；Web 端受 CORS 限制，暂不支持跨域自定义请求头。",
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 30, anchor: { left: 24, right: 24, top: 370 } },
        ),
        ui.place(
          ui.text("momo · Micro App  ·  开放 API 实验", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, anchor: { left: 24, right: 24, top: 408 } },
        ),
      ]),
    },
    {
      title: "用户",
      content: ui.column([
        ui.place(ui.surface("canvas"), {
          anchor: { left: 0, right: 0, top: 0, bottom: 0 },
        }),
        ui.place(ui.surface("paper"), {
          height: 144,
          anchor: { left: 24, right: 24, top: 72 },
        }),
        ui.place(ui.surface("paper"), {
          height: 196,
          anchor: { left: 24, right: 24, top: 222 },
        }),
        ui.place(
          ui.text("PROFILE · 学习账号", {
            font: "uiSans",
            size: 12,
            weight: "regular",
            lineHeight: 14,
          }),
          { height: 16, width: 320, anchor: { left: 24, top: 16 } },
        ),
        ui.place(ui.surface("terracotta"), {
          height: 32,
          width: 4,
          anchor: { left: 12, top: 34 },
        }),
        ui.place(
          ui.text("用户", {
            font: "uiSans",
            size: 24,
            weight: "regular",
            lineHeight: 32,
          }),
          { height: 32, width: 240, anchor: { left: 24, top: 34 } },
        ),
        ui.place(
          ui.button("新建用户", {
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
            onClick: () => {
              if (userCount.value >= 5) {
                userStatus.value = "最多 5 个用户，请切换到已有用户。";
              } else {
                let next = userCount.value + 1;
                userCount.value = next;
                config.setNumber("userCount", next);
                activeUser.value = next;
                config.setNumber("activeUser", next);
                ns.value = `u${next}:`;
                userName.value = `用户 ${next}`;
                nameDraft.value = userName.value;
                tokenDraft.value = "";
                config.setText(`${ns.value}name`, userName.value);
                audio.stop();
                mode.value = 0;
                phase.value = 0;
                idx.value = 0;
                curWord.value = "";
                scanFrom.value = 0;
                roundDone.value = 0;
                roundTotal.value = 0;
                waiting.value = 0;
                reviewIdx.value = 0;
                resetArmed.value = 0;
                syncStatus.value = "";
                syncTotal.value = 0;
                syncDone.value = 0;
                syncWords.value = "";
                syncImported.value = 0;
                mmaFinished.value = 0;
                mmaTotal.value = 0;
                mmaMinutes.value = 0;
                syncVocIds.value = "";
                syncLookup.value = "";
                syncUser.value = 0;
                batchUser.value = 0;
                userStatus.value = `已创建 用户 ${next}，请填写名称和 API Key。`;
                refresh.value++;
              }
            },
          }),
          { width: 108, height: 32, anchor: { right: 44, top: 34 } },
        ),
        ui.place(
          ui.text("当前用户资料", {
            font: "uiSans",
            size: 14,
            weight: "regular",
            lineHeight: 18,
          }),
          { height: 20, width: 200, anchor: { left: 44, top: 82 } },
        ),
        ui.place(
          ui.button("保存", {
            textStyle: { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
            onClick: () => {
              let name = nameDraft.value;
              if (name === "") {
                if (activeUser.value === 1) {
                  name = "默认用户";
                } else {
                  name = `用户 ${activeUser.value}`;
                }
              }
              config.setText(`${ns.value}name`, name);
              config.setText(`${ns.value}maimemoToken`, tokenDraft.value);
              userName.value = name;
              nameDraft.value = name;
              userStatus.value = "已保存当前用户的名称和 API Key。";
              refresh.value++;
            },
          }),
          { width: 72, height: 26, anchor: { right: 44, top: 78 } },
        ),
        ui.place(
          ui.input(
            bind(() => nameDraft.value),
            {
              placeholder: "用户名",
              textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
              onChange: (s) => {
                nameDraft.value = s;
                userStatus.value = "";
              },
            },
          ),
          { height: 34, anchor: { left: 44, right: 44, top: 104 } },
        ),
        ui.place(
          ui.input(
            bind(() => tokenDraft.value),
            {
              placeholder: "墨墨开放 API Token",
              textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
              onChange: (s) => {
                tokenDraft.value = s;
                userStatus.value = "";
              },
            },
          ),
          { height: 34, anchor: { left: 44, right: 44, top: 144 } },
        ),
        ui.place(
          ui.text(
            bind(() => {
              if (userStatus.value === "") {
                return "名称和 Token 只保存到当前用户。";
              }
              return userStatus.value;
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 16, anchor: { left: 44, right: 44, top: 184 } },
        ),
        ui.place(
          ui.text("切换用户", {
            font: "uiSans",
            size: 18,
            weight: "regular",
            lineHeight: 24,
          }),
          { height: 24, width: 240, anchor: { left: 44, top: 230 } },
        ),
        ui.place(
          ui.text(
            bind(() => `共 ${userCount.value} / 5 个用户`),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { height: 14, width: 180, anchor: { right: 44, top: 236 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("name");
                if (name === "") {
                  name = "默认用户";
                }
                return name;
              }),
              {
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  if (activeUser.value !== 1) {
                    activeUser.value = 1;
                    config.setNumber("activeUser", 1);
                    ns.value = "";
                    userName.value = config.getText(`${ns.value}name`);
                    if (userName.value === "") {
                      userName.value = "默认用户";
                    }
                    nameDraft.value = userName.value;
                    tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                    audio.stop();
                    mode.value = 0;
                    phase.value = 0;
                    idx.value = 0;
                    curWord.value = "";
                    scanFrom.value = 0;
                    roundDone.value = 0;
                    roundTotal.value = 0;
                    waiting.value = 0;
                    reviewIdx.value = 0;
                    resetArmed.value = 0;
                    syncStatus.value = "";
                    syncTotal.value = 0;
                    syncDone.value = 0;
                    syncWords.value = "";
                    syncImported.value = 0;
                    mmaFinished.value = 0;
                    mmaTotal.value = 0;
                    mmaMinutes.value = 0;
                    syncVocIds.value = "";
                    syncLookup.value = "";
                    syncUser.value = 0;
                    batchUser.value = 0;
                    userStatus.value = `已切换到 ${userName.value}。`;
                    refresh.value++;
                  }
                },
              },
            ),
            bind(() => {
              if (userCount.value < 1) {
                return false;
              }
              return activeUser.value !== 1;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 256 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("name");
                if (name === "") {
                  name = "默认用户";
                }
                return `${name}  ·  当前`;
              }),
              {
                variant: "selected",
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  userStatus.value = "已在当前用户，可直接编辑资料。";
                  refresh.value++;
                },
              },
            ),
            bind(() => {
              if (userCount.value < 1) {
                return false;
              }
              return activeUser.value === 1;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 256 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u2:name");
                if (name === "") {
                  name = "用户 2";
                }
                return name;
              }),
              {
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  if (activeUser.value !== 2) {
                    activeUser.value = 2;
                    config.setNumber("activeUser", 2);
                    ns.value = "u2:";
                    userName.value = config.getText(`${ns.value}name`);
                    if (userName.value === "") {
                      userName.value = "用户 2";
                    }
                    nameDraft.value = userName.value;
                    tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                    audio.stop();
                    mode.value = 0;
                    phase.value = 0;
                    idx.value = 0;
                    curWord.value = "";
                    scanFrom.value = 0;
                    roundDone.value = 0;
                    roundTotal.value = 0;
                    waiting.value = 0;
                    reviewIdx.value = 0;
                    resetArmed.value = 0;
                    syncStatus.value = "";
                    syncTotal.value = 0;
                    syncDone.value = 0;
                    syncWords.value = "";
                    syncImported.value = 0;
                    mmaFinished.value = 0;
                    mmaTotal.value = 0;
                    mmaMinutes.value = 0;
                    syncVocIds.value = "";
                    syncLookup.value = "";
                    syncUser.value = 0;
                    batchUser.value = 0;
                    userStatus.value = `已切换到 ${userName.value}。`;
                    refresh.value++;
                  }
                },
              },
            ),
            bind(() => {
              if (userCount.value < 2) {
                return false;
              }
              return activeUser.value !== 2;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 288 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u2:name");
                if (name === "") {
                  name = "用户 2";
                }
                return `${name}  ·  当前`;
              }),
              {
                variant: "selected",
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  userStatus.value = "已在当前用户，可直接编辑资料。";
                  refresh.value++;
                },
              },
            ),
            bind(() => {
              if (userCount.value < 2) {
                return false;
              }
              return activeUser.value === 2;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 288 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u3:name");
                if (name === "") {
                  name = "用户 3";
                }
                return name;
              }),
              {
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  if (activeUser.value !== 3) {
                    activeUser.value = 3;
                    config.setNumber("activeUser", 3);
                    ns.value = "u3:";
                    userName.value = config.getText(`${ns.value}name`);
                    if (userName.value === "") {
                      userName.value = "用户 3";
                    }
                    nameDraft.value = userName.value;
                    tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                    audio.stop();
                    mode.value = 0;
                    phase.value = 0;
                    idx.value = 0;
                    curWord.value = "";
                    scanFrom.value = 0;
                    roundDone.value = 0;
                    roundTotal.value = 0;
                    waiting.value = 0;
                    reviewIdx.value = 0;
                    resetArmed.value = 0;
                    syncStatus.value = "";
                    syncTotal.value = 0;
                    syncDone.value = 0;
                    syncWords.value = "";
                    syncImported.value = 0;
                    mmaFinished.value = 0;
                    mmaTotal.value = 0;
                    mmaMinutes.value = 0;
                    syncVocIds.value = "";
                    syncLookup.value = "";
                    syncUser.value = 0;
                    batchUser.value = 0;
                    userStatus.value = `已切换到 ${userName.value}。`;
                    refresh.value++;
                  }
                },
              },
            ),
            bind(() => {
              if (userCount.value < 3) {
                return false;
              }
              return activeUser.value !== 3;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 320 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u3:name");
                if (name === "") {
                  name = "用户 3";
                }
                return `${name}  ·  当前`;
              }),
              {
                variant: "selected",
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  userStatus.value = "已在当前用户，可直接编辑资料。";
                  refresh.value++;
                },
              },
            ),
            bind(() => {
              if (userCount.value < 3) {
                return false;
              }
              return activeUser.value === 3;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 320 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u4:name");
                if (name === "") {
                  name = "用户 4";
                }
                return name;
              }),
              {
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  if (activeUser.value !== 4) {
                    activeUser.value = 4;
                    config.setNumber("activeUser", 4);
                    ns.value = "u4:";
                    userName.value = config.getText(`${ns.value}name`);
                    if (userName.value === "") {
                      userName.value = "用户 4";
                    }
                    nameDraft.value = userName.value;
                    tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                    audio.stop();
                    mode.value = 0;
                    phase.value = 0;
                    idx.value = 0;
                    curWord.value = "";
                    scanFrom.value = 0;
                    roundDone.value = 0;
                    roundTotal.value = 0;
                    waiting.value = 0;
                    reviewIdx.value = 0;
                    resetArmed.value = 0;
                    syncStatus.value = "";
                    syncTotal.value = 0;
                    syncDone.value = 0;
                    syncWords.value = "";
                    syncImported.value = 0;
                    mmaFinished.value = 0;
                    mmaTotal.value = 0;
                    mmaMinutes.value = 0;
                    syncVocIds.value = "";
                    syncLookup.value = "";
                    syncUser.value = 0;
                    batchUser.value = 0;
                    userStatus.value = `已切换到 ${userName.value}。`;
                    refresh.value++;
                  }
                },
              },
            ),
            bind(() => {
              if (userCount.value < 4) {
                return false;
              }
              return activeUser.value !== 4;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 352 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u4:name");
                if (name === "") {
                  name = "用户 4";
                }
                return `${name}  ·  当前`;
              }),
              {
                variant: "selected",
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  userStatus.value = "已在当前用户，可直接编辑资料。";
                  refresh.value++;
                },
              },
            ),
            bind(() => {
              if (userCount.value < 4) {
                return false;
              }
              return activeUser.value === 4;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 352 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u5:name");
                if (name === "") {
                  name = "用户 5";
                }
                return name;
              }),
              {
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  if (activeUser.value !== 5) {
                    activeUser.value = 5;
                    config.setNumber("activeUser", 5);
                    ns.value = "u5:";
                    userName.value = config.getText(`${ns.value}name`);
                    if (userName.value === "") {
                      userName.value = "用户 5";
                    }
                    nameDraft.value = userName.value;
                    tokenDraft.value = config.getText(`${ns.value}maimemoToken`);
                    audio.stop();
                    mode.value = 0;
                    phase.value = 0;
                    idx.value = 0;
                    curWord.value = "";
                    scanFrom.value = 0;
                    roundDone.value = 0;
                    roundTotal.value = 0;
                    waiting.value = 0;
                    reviewIdx.value = 0;
                    resetArmed.value = 0;
                    syncStatus.value = "";
                    syncTotal.value = 0;
                    syncDone.value = 0;
                    syncWords.value = "";
                    syncImported.value = 0;
                    mmaFinished.value = 0;
                    mmaTotal.value = 0;
                    mmaMinutes.value = 0;
                    syncVocIds.value = "";
                    syncLookup.value = "";
                    syncUser.value = 0;
                    batchUser.value = 0;
                    userStatus.value = `已切换到 ${userName.value}。`;
                    refresh.value++;
                  }
                },
              },
            ),
            bind(() => {
              if (userCount.value < 5) {
                return false;
              }
              return activeUser.value !== 5;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 384 } },
        ),
        ui.place(
          ui.show(
            ui.button(
              bind(() => {
                if (refresh.value < 0) {
                  return "";
                }
                let name = config.getText("u5:name");
                if (name === "") {
                  name = "用户 5";
                }
                return `${name}  ·  当前`;
              }),
              {
                variant: "selected",
                textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
                onClick: () => {
                  userStatus.value = "已在当前用户，可直接编辑资料。";
                  refresh.value++;
                },
              },
            ),
            bind(() => {
              if (userCount.value < 5) {
                return false;
              }
              return activeUser.value === 5;
            }),
          ),
          { height: 30, anchor: { left: 44, right: 44, top: 384 } },
        ),
      ]),
    },
  ]),
);

<script setup lang="ts">
import { withBase } from 'vitepress'
import Live2DStage from './Live2DStage.vue'

const stories = [
  {
    number: '01',
    title: '在场',
    heading: '桌面的一角，是她的位置。',
    body: '眨眼、注视、被轻轻点一下的回应。让形象成为相处的入口。',
  },
  {
    number: '02',
    title: '接话',
    heading: '你开口，她就听。',
    body: '文字与语音随时切换。插话时停下来，而不是把上一句强行说完。',
  },
  {
    number: '03',
    title: '熟悉',
    heading: '一点点，成为你的 momo。',
    body: '从人设与对话记录开始，再长出可查看、可修改的记忆。',
  },
]

const blueprintLinks = [
  ['总体架构', '/guide/architecture'],
  ['桌面宿主', '/guide/desktop'],
  ['操作路径', '/guide/workflows'],
  ['通信协议', '/guide/protocol'],
]
</script>

<template>
  <main class="momo-home">
    <section class="momo-hero" aria-labelledby="momo-title">
      <div class="momo-intro">
        <h1 id="momo-title"><span>今天，也有</span><span><em>momo</em> 在。</span></h1>
        <p class="hero-copy">让桌面多一个，懂得回应的存在。</p>
        <p class="hero-detail">说句话，敲几个字。<br>她接得住，也懂得安静陪着。</p>
        <div class="hero-links"><a :href="withBase('/guide/')">认识 momo <span aria-hidden="true">→</span></a><a :href="withBase('/guide/architecture')">打开设计图谱</a></div>
      </div>
      <div class="momo-scene"><Live2DStage /></div>
    </section>

    <section class="momo-story" aria-labelledby="story-title">
      <div class="story-heading">
        <div>
          <p class="eyebrow">01 / 相处，而不只是问答</p>
          <h2 id="story-title">有回应，也有留白。</h2>
        </div>
        <svg class="story-orbit" viewBox="0 0 280 120" aria-hidden="true">
          <ellipse cx="140" cy="60" rx="118" ry="34" />
          <circle cx="224" cy="36" r="5" />
          <path d="M40 72c28-18 53-18 78 0s50 18 75 0 47-18 67-2" />
        </svg>
      </div>
      <div class="story-rows">
        <article v-for="story in stories" :key="story.number" class="story-row">
          <p class="story-number">{{ story.number }}</p>
          <h3>{{ story.title }}</h3>
          <div><strong>{{ story.heading }}</strong><p>{{ story.body }}</p></div>
        </article>
      </div>
      <svg class="story-wave" viewBox="0 0 840 88" aria-hidden="true">
        <path d="M0 44h82l18-24 24 50 28-54 26 28h98l20-16 20 31 22-41 22 26h112l18-26 25 52 29-48 20 22h145" />
      </svg>
    </section>

    <section class="momo-blueprint" aria-labelledby="blueprint-title">
      <div class="blueprint-copy">
        <p class="eyebrow">02 / BLUEPRINT</p>
        <h2 id="blueprint-title"><span>把陪伴拆成</span><span>可以实现的边界。</span></h2>
        <p>呈现负责在场，编排守住回合，模型提供能力。每一层都能替换，也都不越权。</p>
      </div>
      <div class="blueprint-flow" aria-label="桌面呈现到模型能力的三层架构">
        <span>桌面呈现</span><i aria-hidden="true"></i><span>回合编排</span><i aria-hidden="true"></i><span>模型能力</span>
      </div>
      <nav class="blueprint-links" aria-label="设计文档入口">
        <a v-for="([label, link], index) in blueprintLinks" :key="link" :href="withBase(link)"><span>{{ String(index + 1).padStart(2, '0') }}</span>{{ label }}<b aria-hidden="true">↗</b></a>
      </nav>
    </section>

    <section class="momo-road" aria-labelledby="road-title">
      <div>
        <p class="eyebrow">03 / ROAD</p>
      </div>
      <ol>
      </ol>
      <div class="road-end">
        <a :href="withBase('/guide/roadmap')">阅读完整路线 <span aria-hidden="true">→</span></a>
      </div>
    </section>

    <footer class="momo-home-footer">
      <span>momo · 午后桌面</span>
    </footer>
  </main>
</template>

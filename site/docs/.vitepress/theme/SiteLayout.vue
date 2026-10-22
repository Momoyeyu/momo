<script setup lang="ts">
import { useData } from 'vitepress'
import ProductHome from './ProductHome.vue'
import HelpPage from './HelpPage.vue'
const { page } = useData()
</script>
<template><ProductHome v-if="/^(?:(?:en|ja)\/)?index\.md$/.test(page.relativePath)" /><HelpPage v-else /></template>

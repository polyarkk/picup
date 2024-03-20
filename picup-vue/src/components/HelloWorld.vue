<script setup>
import { ref } from 'vue'

defineProps({
  
})

</script>

<template>
  <var-row :gutter="[10, 10]">
    <var-col :span="24">
      <var-paper class="paper" :elevation="2" :width="1200" :height="80">
        <var-cell>hello</var-cell>
      </var-paper>
    </var-col>
    <var-col :span="24">
      <var-paper class="paper" :elevation="2" :width="1200" :height="500">
        <var-cell>hello</var-cell>
      </var-paper>
    </var-col>
  </var-row>
</template>

<style scoped>
.read-the-docs {
  color: #888;
}
.paper {
  background-color: aliceblue;
}
</style>

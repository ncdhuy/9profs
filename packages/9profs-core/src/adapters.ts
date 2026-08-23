import type {
  AssistantDefinition,
  AssistantRegistry,
  SkillDefinition,
  SkillProvider,
} from './types'
import type { CoreTransport } from './transport'

export function createCoreAssistantRegistry(transport: CoreTransport): AssistantRegistry {
  const map = (
    assistant: Awaited<ReturnType<CoreTransport['assistant']>>,
  ): AssistantDefinition => ({
    id: assistant.id,
    description: assistant.description,
  })

  return {
    listAssistants: async () => (await transport.assistants()).map(map),
    resolveAssistant: async (id) => {
      try {
        return map(await transport.assistant(id))
      } catch {
        return undefined
      }
    },
  }
}

export function createCoreSkillProvider(transport: CoreTransport): SkillProvider {
  const map = (skill: Awaited<ReturnType<CoreTransport['skill']>>): SkillDefinition => ({
    id: skill.id,
    description: skill.description,
  })

  return {
    listSkills: async () => (await transport.skills()).skills.map(map),
    resolveSkill: async (id) => {
      try {
        return map(await transport.skill(id))
      } catch {
        return undefined
      }
    },
  }
}

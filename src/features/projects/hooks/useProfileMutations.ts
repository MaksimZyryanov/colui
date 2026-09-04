import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createProfile, removeProfile, updateProfile } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import { inventoryKeys } from '../../../features/runtime/query-keys';
import type { ProfileDraft, RemoveProfileRequest, UpdateProfileRequest } from '../../../ipc/types';
export function useProfileMutations() {
  const client = useQueryClient();
  const create = useMutation({ mutationFn: (draft: ProfileDraft) => createProfile(draft), onSuccess: () => client.invalidateQueries({ queryKey: projectKeys.list() }) });
  const update = useMutation({ mutationFn: (request: UpdateProfileRequest) => updateProfile(request), onSuccess: profile => { void client.invalidateQueries({ queryKey: projectKeys.list() }); void client.invalidateQueries({ queryKey: projectKeys.detail(profile.id) }); void client.invalidateQueries({ queryKey: projectKeys.definition(profile.id) }); } });
  const remove = useMutation({ mutationFn: (request: RemoveProfileRequest) => removeProfile(request), onSuccess: (_data, request) => { void client.invalidateQueries({ queryKey: projectKeys.list() }); client.removeQueries({ queryKey: projectKeys.detail(request.profileId) }); client.removeQueries({ queryKey: projectKeys.status(request.profileId) }); client.removeQueries({ queryKey: projectKeys.definition(request.profileId) }); client.removeQueries({ queryKey: inventoryKeys.all() }); } });
  return { create, update, remove };
}
